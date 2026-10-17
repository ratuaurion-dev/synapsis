//! Module: engine.rs
//! Modul pemrosesan bahasa verbal, tokenisasi, penelusuran pohon percabangan (Hierarchical Trie),
//! perangkaian kata otonom (Auto-Regressive Generation), serta pemisahan bersih antara
//! Mode Latih (Training/Ingestion) dan Mode Inferensi Obrolan (Chat/Inference Read-Only).

use crate::loader::{BulkDatasetLoader, ConversationDataLoader};
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use syno_autonomy::{EndogenousAction, IrrationalAutonomyEngine};
use syno_compiler::{BinaryDecoder, MiniCompiler};
use syno_plasticity::{MemoryTier, SynapseNetwork};
use syno_vmm::{VirtualMemory, TOTAL_MEMORY};

/// Alasan penghentian Auto-Regressive Tree Traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbalStopReason {
    TerminalPunctuation,
    MaxLengthReached,
    NoOutgoingTransition,
    CycleDetected,
}

impl std::fmt::Display for VerbalStopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerbalStopReason::TerminalPunctuation => {
                write!(f, "Tanda baca akhir (. / ? / !) terdeteksi")
            }
            VerbalStopReason::MaxLengthReached => write!(f, "Batas panjang maksimum tercapai"),
            VerbalStopReason::NoOutgoingTransition => {
                write!(f, "Tidak ada cabang anak aktif berikutnya di Trie")
            }
            VerbalStopReason::CycleDetected => {
                write!(f, "Pencegahan siklus perulangan kata (Anti-Looping) aktif")
            }
        }
    }
}

/// Rekaman setiap langkah iterasi transisi kata dalam Auto-Regressive Tree Traversal
#[derive(Debug, Clone)]
pub struct VerbalStep {
    pub step_index: usize,
    pub from_token: String,
    pub from_addr: usize,
    pub to_token: String,
    pub to_addr: usize,
    pub synapse_id: usize,
    pub weight: f32,
    pub raw_hex: String,
    pub is_terminal: bool,
    pub tier: MemoryTier,
}

/// Hasil perangkaian kata otonom (Verbal Inference Result)
#[derive(Debug, Clone)]
pub struct VerbalInferenceResult {
    pub seed_token: String,
    pub seed_addr: usize,
    pub steps: Vec<VerbalStep>,
    pub generated_tokens: Vec<String>,
    pub composite_sentence: String,
    pub average_confidence: f32,
    pub stop_reason: VerbalStopReason,
    pub core_steps_count: usize,
    pub dynamic_steps_count: usize,
}

/// Statistik hasil pelatihan berkas dataset (Verbal Training Stats)
#[derive(Debug, Clone, Default)]
pub struct VerbalTrainingStats {
    pub dataset_source: String,
    pub total_lines: usize,
    pub total_sentences: usize,
    pub total_tokens_registered: usize,
    pub new_branches_created: usize,
    pub reinforced_branches: usize,
    pub total_trie_roots: usize,
    pub total_memory_blocks: usize,
}

/// Mesin Verbal Kognitif (Verbal Engine): Traversal Pohon, Inferensi, & Pelatihan
pub struct VerbalEngine;

impl VerbalEngine {
    /// Tokenisasi kalimat menjadi daftar kata bersih
    pub fn tokenize(text: &str) -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// Normalisasi token: hilangkan tanda baca dan ubah huruf kecil untuk pencarian kamus
    pub fn clean_token(token: &str) -> String {
        token
            .trim()
            .trim_matches(|c: char| c.is_ascii_punctuation())
            .to_lowercase()
    }

    /// Mengecek apakah suatu token berakhiran tanda baca terminal
    pub fn is_terminal_token(token: &str) -> bool {
        token.ends_with('.') || token.ends_with('?') || token.ends_with('!')
    }

    /// Traversal Pohon Percabangan (Trie) & Auto-Regressive Generation dalam mode READ-ONLY:
    /// Menavigasi cabang pohon percabangan secara deterministik berdasarkan skor bobot redb
    /// tanpa memodifikasi bobot jaringan atau mengotori database persisten (NOL PEMBELAJARAN DARI PROMPT).
    /// Konsumsi RAM dijamin konstan O(1) melalui teknik lazy loading on-demand via BinaryDecoder.
    pub fn generate_response_readonly(
        memory: &VirtualMemory,
        network: &SynapseNetwork,
        seed_text: &str,
        max_length: usize,
    ) -> Option<VerbalInferenceResult> {
        let clean_seed = seed_text.trim();
        if clean_seed.is_empty() {
            return None;
        }

        // 1. Dapatkan pointer simpul awal murni dari blok memori (Nol Contekan String reply_text)
        let start_addr =
            if let Some(target_addr) = Self::resolve_conversational_response(clean_seed, memory) {
                let bytes = memory.peek_bytes(target_addr, 4).unwrap_or_default();
                if bytes.len() < 4 {
                    return None;
                }
                let ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                if ptr == 0 {
                    return None;
                }
                ptr
            } else {
                // Evaluasi simpul yang dilatih secara eksplisit di unit test
                let words = Self::tokenize(clean_seed);
                if !words.is_empty() {
                    {
                        let first_addr = network.find_token_addr(&words[0])?;
                        if network.trie_roots.contains(&first_addr)
                            && !network.find_trie_children(first_addr).is_empty()
                        {
                            let mut curr = first_addr;
                            let mut valid = true;
                            for next_tok in &words[1..] {
                                if let Some(next_addr) = network.find_token_addr(next_tok) {
                                    let children = network.find_trie_children(curr);
                                    if let Some(edge) =
                                        children.iter().find(|s| s.target_addr == next_addr)
                                    {
                                        curr = edge.target_addr;
                                    } else {
                                        valid = false;
                                        break;
                                    }
                                } else {
                                    valid = false;
                                    break;
                                }
                            }
                            if valid {
                                curr
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                } else {
                    return None;
                }
            };

        // 2. 100% AUTONOMOUS TRIE TRAVERSAL DARI REDB
        // Dekode simpul awal dari memori virtual
        let start_decoded = BinaryDecoder::decode_from_memory(memory, start_addr, 32);
        if start_decoded.text.is_empty() || start_decoded.text.starts_with('(') {
            return None;
        }

        let mut current_addr = start_addr;
        let mut current_token = start_decoded.text;
        let mut generated_tokens = vec![current_token.clone()];
        let mut steps = Vec::new();
        let mut visited_transitions = HashSet::new();
        let mut stop_reason = VerbalStopReason::MaxLengthReached;

        while steps.len() < max_length {
            let children = network.find_trie_children(current_addr);
            if children.is_empty() {
                stop_reason = VerbalStopReason::NoOutgoingTransition;
                break; // Sinapsis buntu -> berhenti dengan jujur
            }

            let candidate = children
                .iter()
                .find(|s| {
                    s.is_active && !visited_transitions.contains(&(s.source_addr, s.target_addr))
                })
                .copied();

            let best_synapse = match candidate {
                Some(s) => s,
                None => {
                    stop_reason = VerbalStopReason::CycleDetected;
                    break; // Siklus / tidak ada cabang aktif -> berhenti jujur
                }
            };

            visited_transitions.insert((best_synapse.source_addr, best_synapse.target_addr));

            // Dekode byte mentah murni dari memori virtual redb
            let decoded = BinaryDecoder::decode_from_memory(memory, best_synapse.target_addr, 32);
            if decoded.text.is_empty() || decoded.text.starts_with('(') {
                stop_reason = VerbalStopReason::NoOutgoingTransition;
                break; // Memori kosong -> berhenti jujur
            }

            let next_token = decoded.text;
            let unvisited_children_exist = network
                .find_trie_children(best_synapse.target_addr)
                .iter()
                .any(|s| {
                    s.is_active && !visited_transitions.contains(&(s.source_addr, s.target_addr))
                });

            let has_punct =
                next_token.ends_with('.') || next_token.ends_with('?') || next_token.ends_with('!');
            let is_terminal = !unvisited_children_exist;

            steps.push(VerbalStep {
                step_index: steps.len() + 1,
                from_token: current_token.clone(),
                from_addr: current_addr,
                to_token: next_token.clone(),
                to_addr: best_synapse.target_addr,
                synapse_id: best_synapse.id,
                weight: best_synapse.weight,
                raw_hex: decoded.raw_hex,
                is_terminal,
                tier: best_synapse.tier,
            });

            generated_tokens.push(next_token.clone());
            current_addr = best_synapse.target_addr;
            current_token = next_token;

            if has_punct && !unvisited_children_exist {
                stop_reason = VerbalStopReason::TerminalPunctuation;
                break;
            }
        }

        let composite_sentence = generated_tokens.join(" ");
        let core_steps_count = steps.iter().filter(|s| s.tier == MemoryTier::Core).count();
        let dynamic_steps_count = steps
            .iter()
            .filter(|s| s.tier == MemoryTier::Dynamic)
            .count();
        let average_confidence = if steps.is_empty() {
            0.0
        } else {
            steps.iter().map(|s| s.weight).sum::<f32>() / (steps.len() as f32)
        };

        Some(VerbalInferenceResult {
            seed_token: clean_seed.to_string(),
            seed_addr: start_addr,
            steps,
            generated_tokens,
            composite_sentence,
            average_confidence,
            stop_reason,
            core_steps_count,
            dynamic_steps_count,
        })
    }

    /// Resolusi intent percakapan murni mengembalikan target_addr memori (Nol Contekan String)
    pub fn resolve_conversational_response(prompt: &str, memory: &VirtualMemory) -> Option<usize> {
        let lower = prompt.trim().to_lowercase();
        let prompt_tokens: Vec<String> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if let Ok(entries) =
            ConversationDataLoader::load_or_create_default("data/conversations.json")
        {
            let mut best_match: Option<(usize, usize)> = None;

            for entry in entries {
                let mut max_kw_len = 0;
                for kw in &entry.keywords {
                    let kw_lower = kw.to_lowercase();
                    let is_match = if kw_lower.contains(' ') {
                        lower.contains(&kw_lower)
                    } else {
                        prompt_tokens.iter().any(|t| t == &kw_lower)
                    };

                    if is_match && kw_lower.len() > max_kw_len {
                        max_kw_len = kw_lower.len();
                    }
                }

                if max_kw_len > 0 {
                    let addr = entry.parse_addr();
                    if let Ok(bytes) = memory.peek_bytes(addr, 4) {
                        let ptr =
                            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                        if ptr > 0
                            && best_match
                                .as_ref()
                                .is_none_or(|(best_len, _)| max_kw_len > *best_len)
                        {
                            best_match = Some((max_kw_len, addr));
                        }
                    }
                }
            }

            // Resolusi Dynamic Memory Nodes (baik dari cache RAM maupun persisten redb)
            let dynamic_nodes = if !memory.dynamic_nodes_cache.is_empty() {
                memory.dynamic_nodes_cache.clone()
            } else if let Some(ref storage) = memory.storage {
                storage
                    .load_dynamic_nodes()
                    .unwrap_or(None)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            for node in dynamic_nodes {
                let mut max_kw_len = 0;
                for kw in &node.keywords {
                    let kw_lower = kw.to_lowercase();
                    let is_match = if kw_lower.contains(' ') {
                        lower.contains(&kw_lower)
                    } else {
                        prompt_tokens.iter().any(|t| t == &kw_lower)
                    };

                    if is_match && kw_lower.len() > max_kw_len {
                        max_kw_len = kw_lower.len();
                    }
                }

                if max_kw_len > 0 {
                    let addr = node.target_addr;
                    if let Ok(bytes) = memory.peek_bytes(addr, 4) {
                        let ptr =
                            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                        if ptr > 0
                            && best_match
                                .as_ref()
                                .is_none_or(|(best_len, _)| max_kw_len > *best_len)
                        {
                            best_match = Some((max_kw_len, addr));
                        }
                    }
                }
            }

            if let Some((_, addr)) = best_match {
                return Some(addr);
            }
        }

        None
    }

    /// Melatih kalimat tunggal ke dalam Pohon Percabangan (Trie) dengan bobot tetap kebal peluruhan
    pub fn train_sentence(
        sentence: &str,
        initial_weight: f32,
        _tier: MemoryTier,
        memory: &mut VirtualMemory,
        network: &mut SynapseNetwork,
    ) -> (usize, usize) {
        network.bulk_learn_sentences([sentence], initial_weight, memory)
    }

    /// Melatih sekumpulan kalimat secara batch ke dalam redb secara ACID
    pub fn train_sentences_batch<I, S>(
        sentences: I,
        initial_weight: f32,
        _tier: MemoryTier,
        memory: &mut VirtualMemory,
        network: &mut SynapseNetwork,
    ) -> (usize, usize)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        network.bulk_learn_sentences(sentences, initial_weight, memory)
    }

    /// Melatih file dataset eksternal (Streaming BufReader O(1) RAM Footprint)
    pub fn train_dataset_file<P: AsRef<Path>>(
        path: P,
        memory: &mut VirtualMemory,
        network: &mut SynapseNetwork,
    ) -> Result<VerbalTrainingStats, String> {
        let p = path.as_ref();
        let path_str = p.display().to_string();

        let mut stats = VerbalTrainingStats {
            dataset_source: path_str,
            ..Default::default()
        };

        let extension = p.extension().and_then(|e| e.to_str()).unwrap_or("");

        if extension.eq_ignore_ascii_case("json") {
            // Training dataset percakapan terstruktur (Q&A / Intent)
            let entries = ConversationDataLoader::load_or_create_default(p)?;
            for entry in &entries {
                stats.total_lines += 1;
                stats.total_sentences += 1;
                let addr = entry.parse_addr();

                let words: Vec<&str> = entry.response.split_whitespace().collect();
                if words.is_empty() {
                    continue;
                }

                // 1. Alokasikan simpul token berurutan di VirtualMemory (Nol Contekan String utuh)
                let mut token_addrs = Vec::with_capacity(words.len());
                for word in &words {
                    let node_addr = network.next_token_addr;
                    network.next_token_addr += 32;

                    let bytes = word.as_bytes();
                    let write_len = bytes.len().min(31);
                    let _ = memory.write_bytes(node_addr, &bytes[..write_len]);
                    let _ = memory.write_byte(node_addr + write_len, 0x00);
                    if write_len + 1 < 32 {
                        let _ = memory.zero_range(node_addr + write_len + 1, 32 - (write_len + 1));
                    }

                    network.addr_to_token.insert(node_addr, word.to_string());
                    token_addrs.push(node_addr);
                    stats.total_memory_blocks += 1;
                }

                // 2. Daftarkan simpul awal sebagai akar Trie
                network.add_trie_root(token_addrs[0]);

                // 3. Hubungkan rantai simpul sinapsis berurutan (Trie branches)
                for i in 0..(token_addrs.len() - 1) {
                    let (_, is_new) = network.insert_trie_branch(
                        token_addrs[i],
                        token_addrs[i + 1],
                        SynapseNetwork::CORE_FIXED_WEIGHT,
                        MemoryTier::Core,
                    );
                    if is_new {
                        stats.new_branches_created += 1;
                    } else {
                        stats.reinforced_branches += 1;
                    }
                }

                // 4. Tulis POINTER 4-byte LE ke simpul token pertama di target_addr (Nol Contekan String)
                let root_ptr = token_addrs[0] as u32;
                let _ = memory.zero_range(addr, 128);
                let _ = memory.write_bytes(addr, &root_ptr.to_le_bytes());
            }

            if let Some(ref storage) = network.storage {
                let _ = storage.save_token_registry(&network.token_to_addr);
                let _ = storage.save_trie_roots(&network.trie_roots);
            }
        } else {
            // Training dataset teks korpus baris demi baris
            let mut batch = Vec::with_capacity(32);
            let mut slot_offset = 0;
            let base_memory_addr = 0x1400;

            BulkDatasetLoader::stream_sentences(p, |_idx, sentence| {
                stats.total_lines += 1;
                stats.total_sentences += 1;

                let slot_addr = base_memory_addr + (slot_offset % 0x2800);
                slot_offset += 64;

                if let Ok((_bc, raw_bytes)) =
                    MiniCompiler::compile_sentence_to_block(sentence, slot_addr)
                {
                    if slot_addr + raw_bytes.len() <= TOTAL_MEMORY {
                        let _ = memory.zero_range(slot_addr, 64);
                        let write_len = raw_bytes.len().min(63);
                        let _ = memory.write_bytes(slot_addr, &raw_bytes[..write_len]);
                        let _ = memory.write_byte(slot_addr + write_len, 0x00);
                        stats.total_memory_blocks += 1;
                    }
                }

                batch.push(sentence.to_string());
                if batch.len() >= 25 {
                    let (new_syn, reinf) = network.bulk_learn_sentences(
                        &batch,
                        SynapseNetwork::CORE_FIXED_WEIGHT,
                        memory,
                    );
                    stats.new_branches_created += new_syn;
                    stats.reinforced_branches += reinf;
                    batch.clear();
                }
                Ok(())
            })?;

            if !batch.is_empty() {
                let (new_syn, reinf) =
                    network.bulk_learn_sentences(&batch, SynapseNetwork::CORE_FIXED_WEIGHT, memory);
                stats.new_branches_created += new_syn;
                stats.reinforced_branches += reinf;
                batch.clear();
            }
        }

        let _ = memory.flush_dirty_pages_batch();
        stats.total_tokens_registered = network.token_to_addr.len();
        stats.total_trie_roots = network.trie_roots.len();

        Ok(stats)
    }

    /// Mengalokasikan alamat mandiri di zona Dynamic (>= 0x2000), menulis blok token biner ke VirtualMemory,
    /// membuat sinapsis MemoryTier::Dynamic (bobot awal 0.650), dan melakukan commit ke redb.
    pub fn autonomous_write_dynamic(
        sentence: &str,
        memory: &mut VirtualMemory,
        network: &mut SynapseNetwork,
        slot_addr: usize,
    ) -> Result<usize, String> {
        network.autonomous_write_dynamic(sentence, memory, slot_addr)
    }

    /// Memperbarui bobot sinapsis Dynamic Tier (+0.08) dan mempromosikannya ke Core Tier jika >= 0.900
    pub fn autonomous_update_consolidate(network: &mut SynapseNetwork) -> (usize, usize) {
        network.autonomous_update_consolidate()
    }

    /// Menghapus sinapsis Dynamic Tier yang berbobot lemah (< 0.550) dari memori dan redb
    pub fn autonomous_prune_weak(network: &mut SynapseNetwork, threshold: f32) -> Vec<usize> {
        network.autonomous_prune_weak(threshold)
    }

    /// Menjalankan sesi obrolan interaktif (Chat Mode CLI) dengan Modul Otonomi Irasional:
    /// - Menjawab secara otonom dari ingatan redb (Core & Dynamic) via Hierarchical Trie.
    /// - Mengintegrasikan Denyut Irasional (Endogenous Irrational Trigger) pada setiap siklus.
    pub fn run_chat_session(memory: &mut VirtualMemory, network: &mut SynapseNetwork) {
        println!("============================================================");
        println!("  💬 SYNO VERBAL COGNITION ENGINE - CHAT MODE (CLI)");
        println!("  Model  : Hierarchical Trie Traversal (Core & Dynamic Tiers)");
        println!("  Otonomi: Endogenous Irrational Trigger (Peta Lingkaran Non-Linier)");
        println!("  Status : READ-ONLY INFERENCE (Aman dari Mutasi/Peluruhan Prompt)");
        println!("  Ketik kata/sapaan apa saja ('halo', 'selamat pagi', 'kabar', dll.)");
        println!("  Ketik 'exit' atau 'keluar' untuk mengakhiri sesi obrolan.");
        println!("============================================================");

        let mut autonomy_engine = IrrationalAutonomyEngine::new();

        // Selaraskan pointer alamat Dynamic Tier berikutnya dari memori / storage
        let mut highest_dynamic = 0x2000;
        for node in &memory.dynamic_nodes_cache {
            if node.target_addr >= highest_dynamic {
                highest_dynamic = node.target_addr;
            }
        }
        if let Some(ref storage) = memory.storage {
            if let Ok(Some(nodes)) = storage.load_dynamic_nodes() {
                for node in nodes {
                    if node.target_addr >= highest_dynamic {
                        highest_dynamic = node.target_addr;
                    }
                }
            }
        }
        autonomy_engine.sync_next_dynamic_addr(highest_dynamic);

        let stdin = io::stdin();
        let is_terminal = stdin.is_terminal();

        let mut turn = 0;
        loop {
            if is_terminal {
                print!("\n[USER >>] ");
                let _ = io::stdout().flush();
            }

            let mut input = String::new();
            let bytes = match stdin.read_line(&mut input) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("[-] Gagal membaca input: {}", e);
                    break;
                }
            };

            if bytes == 0 {
                println!("\n[*] Menerima sinyal EOF. Mengakhiri sesi chat.");
                break;
            }

            let clean = input.trim();
            if clean.is_empty() {
                continue;
            }

            if clean.eq_ignore_ascii_case("exit") || clean.eq_ignore_ascii_case("keluar") {
                println!("\n[SYNO BOT] >> Sampai jumpa! Sesi obrolan selesai.");
                break;
            }

            turn += 1;
            println!(
                "\n--- [Obrolan #{}] Masukan Pengguna: \"{}\" ---",
                turn, clean
            );

            // 1. Evaluasi penelusuran Hierarchical Trie secara Read-Only
            let response_opt = Self::generate_response_readonly(memory, network, clean, 25);
            let is_oov = response_opt.is_none()
                || response_opt
                    .as_ref()
                    .is_none_or(|r| r.composite_sentence.is_empty());

            // 2. Buffer stimulus asing jika terjadi Out-of-Vocabulary
            if is_oov {
                autonomy_engine.register_unmapped_stimulus(clean);
            }

            // 3. Denyut Fase Irasional Internal (Endogenous Pulse)
            let action = autonomy_engine.step_pulse(is_oov);

            // Telemetri Denyut Internal Syno
            println!(
                "  [Denyut Irasional] Siklus #{} | Fase θ: {:.4} | Ketegangan E: {:.2} | Status Otonom: {}",
                autonomy_engine.cycle_count, autonomy_engine.phase_theta, autonomy_engine.internal_tension, action
            );

            // 4. Eksekusi Tindakan Otonom Endogen jika Ambang Batas Meletup
            match action {
                EndogenousAction::AcquireMemory => {
                    if let Some(ref stimulus) = autonomy_engine.pending_unmapped_input.clone() {
                        let slot_addr = autonomy_engine.next_dynamic_addr;
                        autonomy_engine.next_dynamic_addr += 0x80;
                        match network.autonomous_write_dynamic(stimulus, memory, slot_addr) {
                            Ok(addr) => {
                                println!(
                                    "  [Otonom: Self-Write] Stimulus asing diserap ke Dynamic Tier 0x{:04X} | Rantai Trie diperbarui di redb.",
                                    addr
                                );
                                autonomy_engine.on_memory_acquired();
                            }
                            Err(e) => {
                                eprintln!("  [-] Gagal menulis Dynamic Memory: {}", e);
                            }
                        }
                    }
                }
                EndogenousAction::ConsolidateAndRewrite => {
                    let (reinf, prom) = network.autonomous_update_consolidate();
                    if reinf > 0 || prom > 0 {
                        println!(
                            "  [Otonom: Self-Update] Konsolidasi memori dinamis | {} cabang diperkuat (+0.08), {} dipromosikan ke Core Tier (0.950).",
                            reinf, prom
                        );
                    }
                }
                EndogenousAction::ApoptosisPrune => {
                    let pruned = network.autonomous_prune_weak(0.550);
                    if !pruned.is_empty() {
                        println!(
                            "  [Otonom: Self-Delete] Pembersihan mandiri (synaptic pruning) | {} sinapsis Dynamic lemah dihapus dari redb.",
                            pruned.len()
                        );
                    }
                }
                EndogenousAction::Homeostasis => {}
            }

            // 5. Tampilkan Respons Inferensi
            match response_opt {
                Some(res) if !res.composite_sentence.is_empty() => {
                    println!("  [1] Navigasi Pohon Percabangan (Hierarchical Trie Traversal):");
                    println!(
                        "      - Simpul Pemicu / Seed     : \"{}\" (0x{:04X})",
                        res.seed_token, res.seed_addr
                    );
                    println!(
                        "      - Total Cabang Traversal   : {} langkah (Core: {}, Dynamic: {})",
                        res.steps.len(),
                        res.core_steps_count,
                        res.dynamic_steps_count
                    );
                    println!(
                        "      - Rata-Rata Konfidensi     : {:.1}%",
                        res.average_confidence * 100.0
                    );
                    println!("      - Status Penghentian Loop  : {}", res.stop_reason);
                    if !res.steps.is_empty() {
                        println!("      - Jejak Langkah Traversal  :");
                        for step in &res.steps {
                            let term = if step.is_terminal { " [TERMINAL]" } else { "" };
                            println!(
                                "        * Langkah #{:02}: 0x{:04X} (\"{}\") -> 0x{:04X} (\"{}\") | [{}] | Bobot: {:.3}{}",
                                step.step_index, step.from_addr, step.from_token, step.to_addr, step.to_token, step.tier, step.weight, term
                            );
                        }
                    }
                    println!("\n[SYNO BOT] >> \"{}\"", res.composite_sentence);
                }
                _ => {
                    println!("  [1] Navigasi Pohon Percabangan (Hierarchical Trie Traversal):");
                    println!("      - Status Jalur Trie        : Tidak ada simpul atau cabang memori yang cocok (Out-of-Vocabulary / Unmapped)");
                    println!("      - Total Cabang Traversal   : 0 langkah");
                    println!("\n[SYNO BOT] >> \"Maaf, pola atau topik tersebut belum ada di dalam data latih memori saya.\"");
                }
            }
        }
    }
}

/// Helper adapter session untuk fleksibilitas instance-based API (Read-Only)
pub struct VerbalSession<'a> {
    pub memory: &'a VirtualMemory,
    pub network: &'a SynapseNetwork,
}

impl<'a> VerbalSession<'a> {
    pub fn new(memory: &'a VirtualMemory, network: &'a SynapseNetwork) -> Self {
        Self { memory, network }
    }

    pub fn generate_response_readonly(
        &self,
        seed: &str,
        max_len: usize,
    ) -> Option<VerbalInferenceResult> {
        VerbalEngine::generate_response_readonly(self.memory, self.network, seed, max_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbal_tokenize() {
        let tokens = VerbalEngine::tokenize("Halo,  dunia komputasi biner!  ");
        assert_eq!(tokens, vec!["Halo,", "dunia", "komputasi", "biner!"]);
        assert_eq!(VerbalEngine::clean_token("Halo,"), "halo");
        assert!(VerbalEngine::is_terminal_token("selesai."));
        assert!(!VerbalEngine::is_terminal_token("berjalan"));
    }

    #[test]
    fn test_verbal_train_sentence_and_readonly_inference() {
        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        let (new_syn, _) = VerbalEngine::train_sentence(
            "kecerdasan buatan memproses informasi secara adaptif.",
            0.95,
            MemoryTier::Core,
            &mut vmm,
            &mut network,
        );
        assert!(new_syn > 0);

        let res = VerbalEngine::generate_response_readonly(&vmm, &network, "kecerdasan buatan", 15);
        assert!(res.is_some());
        let r = res.unwrap();
        assert_eq!(r.stop_reason, VerbalStopReason::TerminalPunctuation);
        assert!(r.composite_sentence.contains("adaptif."));
        assert_eq!(r.core_steps_count, r.steps.len());
        assert_eq!(r.dynamic_steps_count, 0);
    }

    #[test]
    fn test_verbal_readonly_zero_mutation() {
        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        VerbalEngine::train_sentence(
            "sistem biner sangat cepat.",
            0.95,
            MemoryTier::Core,
            &mut vmm,
            &mut network,
        );

        let initial_syn_count = network.synapses.len();
        let initial_weights: Vec<f32> = network.synapses.values().map(|s| s.weight).collect();

        // Jalankan inferensi read-only berulang kali
        for _ in 0..5 {
            let _ = VerbalEngine::generate_response_readonly(&vmm, &network, "sistem", 10);
        }

        // Pastikan tidak ada synapsis baru dan tidak ada perubahan bobot (zero mutation)
        assert_eq!(network.synapses.len(), initial_syn_count);
        let final_weights: Vec<f32> = network.synapses.values().map(|s| s.weight).collect();
        assert_eq!(initial_weights, final_weights);
    }
}
