//! Module: network.rs
//! Modul logika adaptif jaringan sinapsis dengan aturan neuroplastisitas (Hebbian & Pruning)
//! dan sinkronisasi persisten ACID ke embedded database redb.

use crate::tier::MemoryTier;
use std::collections::HashMap;
use syno_vmm::{PersistentStorage, SynapseRecord, VirtualMemory};

#[derive(Debug, Clone)]
pub struct Synapse {
    pub id: usize,
    pub source_addr: usize,
    pub target_addr: usize,
    pub weight: f32, // Bobot kekuatan sinapsis (0.0 ..= 1.0)
    pub access_count: u32,
    pub is_active: bool,
    pub tier: MemoryTier,
}

impl From<SynapseRecord> for Synapse {
    fn from(r: SynapseRecord) -> Self {
        Self {
            id: r.id,
            source_addr: r.source_addr,
            target_addr: r.target_addr,
            weight: r.weight,
            access_count: r.access_count,
            is_active: r.is_active,
            tier: r.tier.into(),
        }
    }
}

impl From<&Synapse> for SynapseRecord {
    fn from(s: &Synapse) -> Self {
        Self {
            id: s.id,
            source_addr: s.source_addr,
            target_addr: s.target_addr,
            weight: s.weight,
            access_count: s.access_count,
            is_active: s.is_active,
            tier: s.tier.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HebbianUpdateResult {
    pub synapse_id: usize,
    pub old_weight: f32,
    pub new_weight: f32,
    pub access_count: u32,
    pub potentiation: f32,
    pub tier: MemoryTier,
}

/// Rekaman peristiwa pembelajaran satu pasangan transisi kata (n-gram Word A -> Word B)
#[derive(Debug, Clone)]
pub struct WordTransitionLearningEvent {
    pub synapse_id: usize,
    pub word_a: String,
    pub addr_a: usize,
    pub word_b: String,
    pub addr_b: usize,
    pub old_weight: f32,
    pub new_weight: f32,
    pub is_new_synapse: bool,
    pub access_count: u32,
    pub tier: MemoryTier,
}

/// Hasil dari pembelajaran online real-time dari pesan input pengguna
#[derive(Debug, Clone)]
pub struct OnlineLearningResult {
    pub input_tokens: Vec<String>,
    pub events: Vec<WordTransitionLearningEvent>,
    pub decayed_count: usize,
    pub active_synapse_ids: Vec<usize>,
}

pub struct SynapseNetwork {
    pub next_id: usize,
    pub synapses: HashMap<usize, Synapse>,
    pub potentiation_rate: f32,
    pub depression_rate: f32,
    pub decay_rate: f32,
    pub pruning_threshold: f32,
    pub min_weight: f32,
    pub storage: Option<PersistentStorage>,
    pub token_to_addr: HashMap<String, usize>,
    pub addr_to_token: HashMap<usize, String>,
    pub next_token_addr: usize,
    pub trie_roots: Vec<usize>,
}

impl SynapseNetwork {
    pub const FIXED_CONFIDENCE_WEIGHT: f32 = 0.90;
    pub const CORE_FIXED_WEIGHT: f32 = 0.95;
    pub const DYNAMIC_FIXED_WEIGHT: f32 = 0.90;

    pub fn new() -> Self {
        Self {
            next_id: 1,
            synapses: HashMap::new(),
            potentiation_rate: 0.0,
            depression_rate: 0.0,
            decay_rate: 0.0,
            pruning_threshold: 0.15,
            min_weight: 0.05,
            storage: None,
            token_to_addr: HashMap::new(),
            addr_to_token: HashMap::new(),
            next_token_addr: 0x1000,
            trie_roots: Vec::new(),
        }
    }

    /// Inisialisasi SynapseNetwork dengan memuat topologi dan bobot yang tersimpan sebelumnya dari redb
    pub fn with_storage(storage: PersistentStorage) -> Result<Self, String> {
        let mut network = Self::new();

        if let Ok(Some(next_id)) = storage.load_next_synapse_id() {
            network.next_id = next_id;
        }

        let persisted_records = storage
            .load_all_synapse_records()
            .map_err(|e| format!("{}", e))?;
        for r in persisted_records {
            let s: Synapse = r.into();
            if s.id >= network.next_id {
                network.next_id = s.id + 1;
            }
            network.synapses.insert(s.id, s);
        }

        if let Ok(Some(token_map)) = storage.load_token_registry() {
            for (token, &addr) in &token_map {
                network.addr_to_token.insert(addr, token.clone());
                if addr >= network.next_token_addr {
                    network.next_token_addr = addr + 32;
                }
            }
            network.token_to_addr = token_map;
        }

        if let Ok(Some(roots)) = storage.load_trie_roots() {
            network.trie_roots = roots;
        } else {
            network.reconstruct_trie_roots();
        }

        network.storage = Some(storage);
        Ok(network)
    }

    /// Rekonstruksi simpul akar Trie dari topologi jaringan
    pub fn reconstruct_trie_roots(&mut self) {
        let mut has_incoming = std::collections::HashSet::new();
        let mut has_outgoing = std::collections::HashSet::new();

        for s in self.synapses.values() {
            if s.is_active {
                has_outgoing.insert(s.source_addr);
                has_incoming.insert(s.target_addr);
            }
        }

        let mut roots = Vec::new();
        for &addr in &has_outgoing {
            if !has_incoming.contains(&addr) && self.addr_to_token.contains_key(&addr) {
                roots.push(addr);
            }
        }

        if roots.is_empty() {
            let mut candidates: Vec<_> = has_outgoing.into_iter().collect();
            candidates.sort_by_key(|&a| std::cmp::Reverse(self.find_trie_children(a).len()));
            roots = candidates.into_iter().take(10).collect();
        }

        roots.sort();
        self.trie_roots = roots;
    }

    /// Menghubungkan storage persisten ke jaringan sinapsis yang sudah ada
    pub fn attach_storage(&mut self, storage: PersistentStorage) -> Result<(), String> {
        if let Ok(Some(next_id)) = storage.load_next_synapse_id() {
            if next_id > self.next_id {
                self.next_id = next_id;
            }
        }
        let persisted_records = storage
            .load_all_synapse_records()
            .map_err(|e| format!("{}", e))?;
        for r in persisted_records {
            let s: Synapse = r.into();
            if s.id >= self.next_id {
                self.next_id = s.id + 1;
            }
            self.synapses.entry(s.id).or_insert(s);
        }

        if let Ok(Some(token_map)) = storage.load_token_registry() {
            for (token, &addr) in &token_map {
                self.addr_to_token
                    .entry(addr)
                    .or_insert_with(|| token.clone());
                if addr >= self.next_token_addr {
                    self.next_token_addr = addr + 32;
                }
            }
            for (k, v) in token_map {
                self.token_to_addr.entry(k).or_insert(v);
            }
        }

        if let Ok(Some(roots)) = storage.load_trie_roots() {
            self.trie_roots = roots;
        } else if self.trie_roots.is_empty() {
            self.reconstruct_trie_roots();
        }

        self.storage = Some(storage);
        Ok(())
    }

    /// Mendaftarkan simpul akar Trie baru dan menyimpannya secara persisten ke database redb
    pub fn add_trie_root(&mut self, root_addr: usize) {
        if !self.trie_roots.contains(&root_addr) {
            self.trie_roots.push(root_addr);
            if let Some(ref storage) = self.storage {
                let _ = storage.save_trie_roots(&self.trie_roots);
            }
        }
    }

    /// Memuat sinapsis tunggal secara on-demand / lazy jika belum ada di RAM
    pub fn load_synapse_on_demand(&mut self, id: usize) -> Option<&Synapse> {
        if !self.synapses.contains_key(&id) {
            if let Some(ref storage) = self.storage {
                if let Ok(Some(r)) = storage.load_synapse_record(id) {
                    self.synapses.insert(id, r.into());
                }
            }
        }
        self.synapses.get(&id)
    }

    /// Mencari ID sinapsis aktif yang menghubungkan source_addr dan target_addr
    pub fn find_synapse(&self, source_addr: usize, target_addr: usize) -> Option<usize> {
        for s in self.synapses.values() {
            if s.is_active && s.source_addr == source_addr && s.target_addr == target_addr {
                return Some(s.id);
            }
        }
        None
    }

    /// Menemukan sinapsis yang sudah ada atau membuat sambungan baru dengan kategori default Core Memory
    pub fn find_or_connect(
        &mut self,
        source_addr: usize,
        target_addr: usize,
        initial_weight: f32,
    ) -> (usize, bool) {
        self.find_or_connect_with_tier(source_addr, target_addr, initial_weight, MemoryTier::Core)
    }

    /// Menemukan atau membuat cabang baru dalam Pohon Percabangan (Trie)
    /// dengan mempertahankan bobot tetap konstan dan mengatur tier memori (Core vs Dynamic).
    pub fn find_or_connect_with_tier(
        &mut self,
        source_addr: usize,
        target_addr: usize,
        initial_weight: f32,
        tier: MemoryTier,
    ) -> (usize, bool) {
        if let Some(existing_id) = self.find_synapse(source_addr, target_addr) {
            if let Some(synapse) = self.synapses.get_mut(&existing_id) {
                if synapse.is_active {
                    synapse.access_count = synapse.access_count.saturating_add(1);
                    if tier == MemoryTier::Core {
                        synapse.tier = MemoryTier::Core;
                        synapse.weight = synapse.weight.clamp(Self::CORE_FIXED_WEIGHT, 1.0);
                    } else {
                        synapse.weight = synapse.weight.clamp(initial_weight, 1.0);
                    }
                    if let Some(ref storage) = self.storage {
                        let record: SynapseRecord = (&*synapse).into();
                        let _ = storage.save_synapse_record(&record);
                    }
                }
            }
            (existing_id, false)
        } else {
            let id = self.connect_with_tier(source_addr, target_addr, initial_weight, tier);
            (id, true)
        }
    }

    /// Alias untuk memasukkan cabang ke Pohon Percabangan (Trie)
    pub fn insert_trie_branch(
        &mut self,
        source_addr: usize,
        target_addr: usize,
        initial_weight: f32,
        tier: MemoryTier,
    ) -> (usize, bool) {
        self.find_or_connect_with_tier(source_addr, target_addr, initial_weight, tier)
    }

    /// Membuat sambungan sinapsis baru dengan kategori default Core Memory
    pub fn connect(
        &mut self,
        source_addr: usize,
        target_addr: usize,
        initial_weight: f32,
    ) -> usize {
        self.connect_with_tier(source_addr, target_addr, initial_weight, MemoryTier::Core)
    }

    /// Membuat sambungan sinapsis baru antara dua alamat memori virtual dengan kategori tier
    pub fn connect_with_tier(
        &mut self,
        source_addr: usize,
        target_addr: usize,
        initial_weight: f32,
        tier: MemoryTier,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let floor_weight = match tier {
            MemoryTier::Core => Self::CORE_FIXED_WEIGHT,
            MemoryTier::Dynamic => 0.0,
        };
        let weight = initial_weight.clamp(0.0, 1.0).max(floor_weight);

        let synapse = Synapse {
            id,
            source_addr,
            target_addr,
            weight,
            access_count: 1,
            is_active: true,
            tier,
        };

        if let Some(ref storage) = self.storage {
            let record: SynapseRecord = (&synapse).into();
            let _ = storage.save_synapse_record(&record);
            let _ = storage.save_next_synapse_id(self.next_id);
        }

        self.synapses.insert(id, synapse);
        id
    }

    /// Memperbarui sinapsis dengan aturan Fixed-Weight Associative Memory
    pub fn update_synapse(&mut self, id: usize) -> Result<HebbianUpdateResult, String> {
        let synapse = self
            .synapses
            .get_mut(&id)
            .ok_or_else(|| format!("Sinapsis ID #{} tidak ditemukan", id))?;

        if !synapse.is_active {
            return Err(format!(
                "Sinapsis ID #{} dalam kondisi inaktif/terputus",
                id
            ));
        }

        let old_weight = synapse.weight;
        synapse.access_count = synapse.access_count.saturating_add(1);

        let min_weight = match synapse.tier {
            MemoryTier::Core => Self::CORE_FIXED_WEIGHT,
            MemoryTier::Dynamic => self.min_weight,
        };
        synapse.weight = old_weight.max(min_weight).min(1.0);

        if let Some(ref storage) = self.storage {
            let record: SynapseRecord = (&*synapse).into();
            let _ = storage.save_synapse_record(&record);
        }

        Ok(HebbianUpdateResult {
            synapse_id: id,
            old_weight,
            new_weight: synapse.weight,
            access_count: synapse.access_count,
            potentiation: synapse.weight - old_weight,
            tier: synapse.tier,
        })
    }

    pub fn update_synapse_hebbian(&mut self, id: usize) -> Result<HebbianUpdateResult, String> {
        self.update_synapse(id)
    }

    pub fn apply_weight_decay(
        &mut self,
        active_synapse_id: Option<usize>,
    ) -> Vec<(usize, f32, f32)> {
        match active_synapse_id {
            Some(id) => self.apply_weight_decay_multiple(&[id]),
            None => self.apply_weight_decay_multiple(&[]),
        }
    }

    pub fn apply_weight_decay_multiple(&mut self, _active_ids: &[usize]) -> Vec<(usize, f32, f32)> {
        Vec::new()
    }

    pub fn disconnect(&mut self, id: usize) -> bool {
        if let Some(synapse) = self.synapses.get_mut(&id) {
            synapse.is_active = false;
            if let Some(ref storage) = self.storage {
                let record: SynapseRecord = (&*synapse).into();
                let _ = storage.save_synapse_record(&record);
            }
            true
        } else {
            false
        }
    }

    pub fn rewire(&mut self, id: usize, new_target_addr: usize) -> Result<(), String> {
        match self.synapses.get_mut(&id) {
            Some(synapse) => {
                synapse.target_addr = new_target_addr;
                synapse.weight = (synapse.weight * 0.9).clamp(0.0, 1.0);
                if let Some(ref storage) = self.storage {
                    let record: SynapseRecord = (&*synapse).into();
                    let _ = storage.save_synapse_record(&record);
                }
                Ok(())
            }
            None => Err(format!("Sinapsis ID #{} tidak ditemukan untuk rewire", id)),
        }
    }

    pub fn pulse(
        &mut self,
        id: usize,
        memory: &mut VirtualMemory,
    ) -> Result<(u8, u8, HebbianUpdateResult), String> {
        let (src, dst, weight) = match self.synapses.get(&id) {
            Some(s) if s.is_active => (s.source_addr, s.target_addr, s.weight),
            Some(_) => {
                return Err(format!(
                    "Sinapsis ID #{} dalam kondisi inaktif/terputus",
                    id
                ))
            }
            None => return Err(format!("Sinapsis ID #{} tidak ditemukan", id)),
        };

        let src_byte = memory
            .read_byte(src)
            .map_err(|e| format!("Gagal membaca sumber sinapsis: {}", e))?;

        let transmitted_val = ((src_byte as f32) * weight).round() as u8;

        let existing_dst = memory.read_byte(dst).unwrap_or(0);
        let blended_val = existing_dst.saturating_add(transmitted_val);
        memory
            .write_byte(dst, blended_val)
            .map_err(|e| format!("Gagal menulis ke target sinapsis: {}", e))?;

        let hebbian_res = self.update_synapse(id)?;

        Ok((src_byte, blended_val, hebbian_res))
    }

    pub fn stimulate(&mut self, id: usize, memory: &mut VirtualMemory) -> Result<(u8, u8), String> {
        let (src_byte, blended_val, _) = self.pulse(id, memory)?;
        Ok((src_byte, blended_val))
    }

    pub fn decay_and_prune(&mut self) -> Vec<usize> {
        let mut pruned_ids = Vec::new();
        let mut modified = Vec::new();

        for (id, synapse) in self.synapses.iter_mut() {
            if synapse.is_active {
                synapse.weight -= self.depression_rate;
                if synapse.weight < self.pruning_threshold {
                    synapse.is_active = false;
                    pruned_ids.push(*id);
                }
                modified.push(synapse.clone());
            }
        }

        if let Some(ref storage) = self.storage {
            let records: Vec<SynapseRecord> = modified.iter().map(Into::into).collect();
            let _ = storage.save_synapse_records_batch(&records);
        }

        pruned_ids
    }

    pub fn register_token(&mut self, token: &str, memory: &mut VirtualMemory) -> usize {
        let clean = token.trim();
        if clean.is_empty() {
            return 0;
        }
        let lower = clean.to_lowercase();
        if let Some(&addr) = self.token_to_addr.get(&lower) {
            return addr;
        }

        let addr = self.next_token_addr;
        self.next_token_addr += 32;

        let bytes = clean.as_bytes();
        let write_len = bytes.len().min(31);
        let _ = memory.write_bytes(addr, &bytes[..write_len]);
        let _ = memory.write_byte(addr + write_len, 0x00);
        if write_len + 1 < 32 {
            let _ = memory.zero_range(addr + write_len + 1, 32 - (write_len + 1));
        }

        self.token_to_addr.insert(lower, addr);
        self.addr_to_token.insert(addr, clean.to_string());

        if let Some(ref storage) = self.storage {
            let _ = storage.save_token_registry(&self.token_to_addr);
        }

        addr
    }

    pub fn find_token_addr(&self, token: &str) -> Option<usize> {
        let raw = token.trim();
        let lower = raw.to_lowercase();
        if let Some(&addr) = self.token_to_addr.get(&lower) {
            return Some(addr);
        }
        let stripped = lower.trim_matches(|c: char| c.is_ascii_punctuation());
        if !stripped.is_empty() {
            if let Some(&addr) = self.token_to_addr.get(stripped) {
                return Some(addr);
            }
        }
        None
    }

    pub fn find_token_by_addr(&self, addr: usize) -> Option<&String> {
        self.addr_to_token.get(&addr)
    }

    pub fn connect_word_transition_with_tier(
        &mut self,
        word_a: &str,
        word_b: &str,
        initial_weight: f32,
        tier: MemoryTier,
        memory: &mut VirtualMemory,
    ) -> (usize, f32) {
        let addr_a = self.register_token(word_a, memory);
        let addr_b = self.register_token(word_b, memory);

        let (id, _) = self.find_or_connect_with_tier(addr_a, addr_b, initial_weight, tier);
        let weight = self
            .synapses
            .get(&id)
            .map(|s| s.weight)
            .unwrap_or(Self::CORE_FIXED_WEIGHT);
        (id, weight)
    }

    pub fn connect_word_transition(
        &mut self,
        word_a: &str,
        word_b: &str,
        initial_weight: f32,
        memory: &mut VirtualMemory,
    ) -> (usize, f32) {
        self.connect_word_transition_with_tier(
            word_a,
            word_b,
            initial_weight,
            MemoryTier::Core,
            memory,
        )
    }

    pub fn learn_sequence_with_tier(
        &mut self,
        sentence: &str,
        initial_weight: f32,
        tier: MemoryTier,
        memory: &mut VirtualMemory,
    ) -> Vec<(usize, String, String, f32)> {
        let words: Vec<&str> = sentence
            .split_whitespace()
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
            .collect();

        let mut transitions = Vec::new();
        if words.len() < 2 {
            if let Some(&first) = words.first() {
                let first_addr = self.register_token(first, memory);
                self.add_trie_root(first_addr);
            }
            return transitions;
        }

        let root_addr = self.register_token(words[0], memory);
        self.add_trie_root(root_addr);

        for i in 0..(words.len() - 1) {
            let w1 = words[i];
            let w2 = words[i + 1];
            let (synapse_id, weight) =
                self.connect_word_transition_with_tier(w1, w2, initial_weight, tier, memory);
            transitions.push((synapse_id, w1.to_string(), w2.to_string(), weight));
        }

        transitions
    }

    pub fn learn_sequence(
        &mut self,
        sentence: &str,
        initial_weight: f32,
        memory: &mut VirtualMemory,
    ) -> Vec<(usize, String, String, f32)> {
        self.learn_sequence_with_tier(sentence, initial_weight, MemoryTier::Core, memory)
    }

    pub fn bulk_learn_sentences<I, S>(
        &mut self,
        sentences: I,
        initial_weight: f32,
        memory: &mut VirtualMemory,
    ) -> (usize, usize)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut new_synapses_count = 0;
        let mut reinforced_count = 0;
        let mut modified_synapses = Vec::new();

        for sentence in sentences {
            let s_ref = sentence.as_ref();
            let words: Vec<&str> = s_ref
                .split_whitespace()
                .map(|w| w.trim())
                .filter(|w| !w.is_empty())
                .collect();

            if words.len() < 2 {
                if let Some(&first) = words.first() {
                    let first_addr = self.register_token(first, memory);
                    self.add_trie_root(first_addr);
                }
                continue;
            }

            let root_addr = self.register_token(words[0], memory);
            self.add_trie_root(root_addr);

            for i in 0..(words.len() - 1) {
                let w1 = words[i];
                let w2 = words[i + 1];

                let addr_a = self.register_token(w1, memory);
                let addr_b = self.register_token(w2, memory);

                if let Some(existing_id) = self.find_synapse(addr_a, addr_b) {
                    if let Some(synapse) = self.synapses.get_mut(&existing_id) {
                        if synapse.is_active {
                            synapse.access_count = synapse.access_count.saturating_add(1);
                            synapse.tier = MemoryTier::Core;
                            synapse.weight = synapse.weight.clamp(Self::CORE_FIXED_WEIGHT, 1.0);
                            modified_synapses.push(synapse.clone());
                            reinforced_count += 1;
                        }
                    }
                } else {
                    let id = self.next_id;
                    self.next_id += 1;
                    let synapse = Synapse {
                        id,
                        source_addr: addr_a,
                        target_addr: addr_b,
                        weight: initial_weight.clamp(0.0, 1.0).max(Self::CORE_FIXED_WEIGHT),
                        access_count: 1,
                        is_active: true,
                        tier: MemoryTier::Core,
                    };
                    self.synapses.insert(id, synapse.clone());
                    modified_synapses.push(synapse);
                    new_synapses_count += 1;
                }
            }
        }

        if let Some(ref storage) = self.storage {
            let records: Vec<SynapseRecord> = modified_synapses.iter().map(Into::into).collect();
            let _ = storage.save_synapse_records_batch(&records);
            let _ = storage.save_next_synapse_id(self.next_id);
            let _ = storage.save_token_registry(&self.token_to_addr);
            let _ = storage.save_trie_roots(&self.trie_roots);
        }

        (new_synapses_count, reinforced_count)
    }

    pub fn learn_user_input_online(
        &mut self,
        input_text: &str,
        initial_weight: f32,
        memory: &mut VirtualMemory,
    ) -> OnlineLearningResult {
        let raw_tokens: Vec<String> = input_text
            .split_whitespace()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();

        let mut events = Vec::new();
        let mut active_synapse_ids = Vec::new();

        if raw_tokens.is_empty() {
            return OnlineLearningResult {
                input_tokens: Vec::new(),
                events,
                decayed_count: 0,
                active_synapse_ids,
            };
        }

        for tok in &raw_tokens {
            self.register_token(tok, memory);
        }

        let first_addr = self.register_token(&raw_tokens[0], memory);
        self.add_trie_root(first_addr);

        for i in 0..(raw_tokens.len().saturating_sub(1)) {
            let w_a = &raw_tokens[i];
            let w_b = &raw_tokens[i + 1];

            let addr_a = self.register_token(w_a, memory);
            let addr_b = self.register_token(w_b, memory);

            let (synapse_id, is_new) =
                self.insert_trie_branch(addr_a, addr_b, initial_weight, MemoryTier::Dynamic);
            let (old_w, new_w, access_cnt, tier) = if let Some(s) = self.synapses.get(&synapse_id) {
                (
                    if is_new { 0.0 } else { s.weight },
                    s.weight,
                    s.access_count,
                    s.tier,
                )
            } else {
                (0.0, Self::DYNAMIC_FIXED_WEIGHT, 1, MemoryTier::Dynamic)
            };

            active_synapse_ids.push(synapse_id);
            events.push(WordTransitionLearningEvent {
                synapse_id,
                word_a: w_a.clone(),
                addr_a,
                word_b: w_b.clone(),
                addr_b,
                old_weight: old_w,
                new_weight: new_w,
                is_new_synapse: is_new,
                access_count: access_cnt,
                tier,
            });
        }

        OnlineLearningResult {
            input_tokens: raw_tokens,
            events,
            decayed_count: 0,
            active_synapse_ids,
        }
    }

    pub fn report_topology(&self) -> String {
        let mut report = String::new();
        report.push_str("  ID  | Sumber -> Target  | Bobot | Akses | Tier    | Status\n");
        report.push_str("  ----+-------------------+-------+-------+---------+----------\n");
        for s in self.synapses.values() {
            let status = if s.is_active { "AKTIF" } else { "PUTUS" };
            report.push_str(&format!(
                "  #{:<2} | 0x{:04X} -> 0x{:04X}  | {:.2}  | {:<5} | {:<7} | {}\n",
                s.id, s.source_addr, s.target_addr, s.weight, s.access_count, s.tier, status
            ));
        }
        report
    }

    /// Mengalokasikan alamat mandiri di zona Dynamic (>= 0x2000), menulis blok token biner ke VirtualMemory,
    /// membuat sinapsis MemoryTier::Dynamic (bobot awal 0.650), dan melakukan commit ke redb.
    pub fn autonomous_write_dynamic(
        &mut self,
        sentence: &str,
        memory: &mut VirtualMemory,
        slot_addr: usize,
    ) -> Result<usize, String> {
        if slot_addr < 0x2000 {
            return Err(format!(
                "Alamat slot 0x{:04X} tidak valid untuk Dynamic Tier (harus >= 0x2000)",
                slot_addr
            ));
        }

        let words: Vec<String> = sentence
            .split_whitespace()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            return Err("Kalimat kosong untuk autonomous write".to_string());
        }

        let mut token_addrs = Vec::with_capacity(words.len());
        for word in &words {
            let node_addr = self.next_token_addr;
            self.next_token_addr += 32;

            let bytes = word.as_bytes();
            let write_len = bytes.len().min(31);
            let _ = memory.write_bytes(node_addr, &bytes[..write_len]);
            let _ = memory.write_byte(node_addr + write_len, 0x00);
            if write_len + 1 < 32 {
                let _ = memory.zero_range(node_addr + write_len + 1, 32 - (write_len + 1));
            }

            let lower = word
                .trim()
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_lowercase();
            if !lower.is_empty() {
                self.token_to_addr.insert(lower, node_addr);
            }
            self.addr_to_token.insert(node_addr, word.clone());
            token_addrs.push(node_addr);
        }

        self.add_trie_root(token_addrs[0]);

        for i in 0..(token_addrs.len().saturating_sub(1)) {
            let src = token_addrs[i];
            let dst = token_addrs[i + 1];
            self.insert_trie_branch(src, dst, 0.650, MemoryTier::Dynamic);
        }

        let root_ptr = token_addrs[0] as u32;
        let _ = memory.zero_range(slot_addr, 128);
        memory
            .write_bytes(slot_addr, &root_ptr.to_le_bytes())
            .map_err(|e| format!("{}", e))?;

        let mut keywords = Vec::new();
        for word in &words {
            let clean = word
                .trim()
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_lowercase();
            if clean.len() >= 2 && !keywords.contains(&clean) {
                keywords.push(clean);
            }
        }

        let dynamic_node = syno_vmm::DynamicMemoryNode {
            target_addr: slot_addr,
            keywords,
            raw_sentence: sentence.to_string(),
        };

        memory
            .dynamic_nodes_cache
            .retain(|n| n.target_addr != slot_addr);
        memory.dynamic_nodes_cache.push(dynamic_node.clone());

        if let Some(ref storage) = self.storage {
            let _ = storage.save_token_registry(&self.token_to_addr);
            let _ = storage.save_trie_roots(&self.trie_roots);
            let mut all_dynamic = storage
                .load_dynamic_nodes()
                .map_err(|e| format!("{}", e))?
                .unwrap_or_default();
            all_dynamic.retain(|n| n.target_addr != slot_addr);
            all_dynamic.push(dynamic_node);
            let _ = storage.save_dynamic_nodes(&all_dynamic);
        }

        let _ = memory.flush_dirty_pages_batch();

        Ok(slot_addr)
    }

    /// Refleksi internal: memperkuat bobot sinapsis Dynamic Tier (+0.08) yang aktif
    /// dan mempromosikannya menjadi Core Tier (bobot 0.950) jika melampaui ambang 0.900,
    /// lalu menyimpannya ke redb secara ACID.
    pub fn autonomous_update_consolidate(&mut self) -> (usize, usize) {
        let mut modified = Vec::new();
        let mut reinforced_count = 0;
        let mut promoted_count = 0;

        for synapse in self.synapses.values_mut() {
            if synapse.is_active && synapse.tier == MemoryTier::Dynamic {
                synapse.weight = (synapse.weight + 0.08).min(1.0);
                reinforced_count += 1;

                if synapse.weight >= 0.900 {
                    synapse.tier = MemoryTier::Core;
                    synapse.weight = Self::CORE_FIXED_WEIGHT; // 0.950
                    promoted_count += 1;
                }

                modified.push(synapse.clone());
            }
        }

        if let Some(ref storage) = self.storage {
            let records: Vec<SynapseRecord> = modified.iter().map(Into::into).collect();
            let _ = storage.save_synapse_records_batch(&records);
        }

        (reinforced_count, promoted_count)
    }

    /// Pembersihan mandiri (synaptic pruning): meluruhkan/menghapus simpul dan sinapsis di Dynamic Tier
    /// yang memiliki bobot lemah (< 0.550) dari SynapseNetwork dan redb.
    pub fn autonomous_prune_weak(&mut self, threshold: f32) -> Vec<usize> {
        let mut to_prune = Vec::new();

        for (id, synapse) in &self.synapses {
            if synapse.tier == MemoryTier::Core {
                continue;
            }
            if synapse.tier == MemoryTier::Dynamic && synapse.weight < threshold {
                to_prune.push(*id);
            }
        }

        for &id in &to_prune {
            self.synapses.remove(&id);
        }

        if let Some(ref storage) = self.storage {
            let _ = storage.delete_synapses_batch(&to_prune);
        }

        to_prune
    }
}

impl Default for SynapseNetwork {
    fn default() -> Self {
        Self::new()
    }
}
