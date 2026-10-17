//! Module: trie.rs
//! Modul penelusuran pohon percabangan hierarkis (Hierarchical Trie Index),
//! prefix matching, dan visualisasi topologi cabang memori.

use crate::network::{Synapse, SynapseNetwork};
use crate::tier::MemoryTier;

impl SynapseNetwork {
    /// Mengambil seluruh cabang anak (outgoing branches) dari suatu node Trie.
    /// Diurutkan secara deterministik:
    /// 1. Ingatan Inti (MemoryTier::Core) didahulukan dibanding Ingatan Dinamis (MemoryTier::Dynamic).
    /// 2. Bobot sinapsis tertinggi.
    /// 3. Jumlah akses (access_count) terbanyak.
    pub fn find_trie_children(&self, node_addr: usize) -> Vec<&Synapse> {
        let mut children: Vec<&Synapse> = self
            .synapses
            .values()
            .filter(|s| s.is_active && s.source_addr == node_addr)
            .collect();

        children.sort_by(|a, b| {
            let tier_order = match (a.tier, b.tier) {
                (MemoryTier::Core, MemoryTier::Dynamic) => std::cmp::Ordering::Less,
                (MemoryTier::Dynamic, MemoryTier::Core) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };
            if tier_order != std::cmp::Ordering::Equal {
                return tier_order;
            }

            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.access_count.cmp(&a.access_count))
                .then_with(|| a.id.cmp(&b.id))
        });

        children
    }

    /// Mengambil seluruh transisi keluar (outgoing branches) dari node sumber
    pub fn find_outgoing_transitions(&self, source_addr: usize) -> Vec<&Synapse> {
        self.find_trie_children(source_addr)
    }

    /// Menemukan cabang anak dengan prioritas tertinggi dari node sumber
    pub fn find_strongest_transition(&self, source_addr: usize) -> Option<&Synapse> {
        self.find_trie_children(source_addr).into_iter().next()
    }

    /// Menavigasi pohon percabangan (Trie) berdasarkan urutan kata yang diberikan.
    /// Mengembalikan alamat node Trie terdalam yang berhasil dicapai (prefix match).
    pub fn match_trie_prefix(&self, tokens: &[&str]) -> Option<usize> {
        if tokens.is_empty() {
            return None;
        }

        let first_token = tokens[0];
        let mut current_addr = self.find_token_addr(first_token)?;

        for &next_token in &tokens[1..] {
            let next_addr = match self.find_token_addr(next_token) {
                Some(a) => a,
                None => break,
            };

            let children = self.find_trie_children(current_addr);
            if let Some(edge) = children.iter().find(|s| s.target_addr == next_addr) {
                current_addr = edge.target_addr;
            } else {
                break;
            }
        }

        Some(current_addr)
    }

    /// Menampilkan ringkasan visual Pohon Percabangan Memori (Hierarchical Trie Index)
    pub fn render_trie_overview(&self, max_roots: usize, max_depth: usize) -> String {
        let mut out = String::new();
        out.push_str("========================================================================================\n");
        out.push_str("  🌳 POHON PERCABANGAN MEMORI HIERARKIS (HIERARCHICAL TRIE INDEX)\n");
        out.push_str("  Model: Fixed-Weight Associative Memory (Core vs Dynamic Tiers)\n");
        out.push_str("========================================================================================\n");

        let mut roots_to_show = self.trie_roots.clone();
        if roots_to_show.is_empty() {
            for &addr in self.addr_to_token.keys() {
                if !self.find_trie_children(addr).is_empty() {
                    roots_to_show.push(addr);
                    if roots_to_show.len() >= max_roots {
                        break;
                    }
                }
            }
        }

        let total_core = self
            .synapses
            .values()
            .filter(|s| s.is_active && s.tier == MemoryTier::Core)
            .count();
        let total_dynamic = self
            .synapses
            .values()
            .filter(|s| s.is_active && s.tier == MemoryTier::Dynamic)
            .count();

        out.push_str(&format!(
            "  * Statistik Topologi: {} Akar Trie | {} Cabang Core (Fixed 0.95) | {} Cabang Dynamic (Fixed 0.90)\n\n",
            roots_to_show.len(), total_core, total_dynamic
        ));

        let shown_roots: Vec<usize> = roots_to_show.iter().copied().take(max_roots).collect();
        let roots_len = shown_roots.len();

        for (r_idx, &root_addr) in shown_roots.iter().enumerate() {
            let root_name = self
                .find_token_by_addr(root_addr)
                .cloned()
                .unwrap_or_else(|| format!("0x{:04X}", root_addr));
            let children = self.find_trie_children(root_addr);
            let is_last_root = r_idx + 1 == roots_len;
            let root_prefix = if is_last_root {
                "└──"
            } else {
                "├──"
            };

            out.push_str(&format!(
                "  {} 🌳 [ROOT] \"{}\" (0x{:04X}) | Percabangan: {} anak\n",
                root_prefix,
                root_name,
                root_addr,
                children.len()
            ));

            self.render_trie_branch_recursive(
                &mut out,
                root_addr,
                if is_last_root { "    " } else { "│   " },
                1,
                max_depth,
            );
        }

        out.push_str("========================================================================================\n");
        out
    }

    fn render_trie_branch_recursive(
        &self,
        out: &mut String,
        current_addr: usize,
        indent: &str,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }

        let children = self.find_trie_children(current_addr);
        let count = children.len();
        for (i, edge) in children.iter().take(4).enumerate() {
            let is_last = i + 1 == count.min(4);
            let branch_sym = if is_last { "└──" } else { "├──" };
            let child_tok = self
                .find_token_by_addr(edge.target_addr)
                .cloned()
                .unwrap_or_else(|| format!("0x{:04X}", edge.target_addr));

            let tier_tag = match edge.tier {
                MemoryTier::Core => "CORE",
                MemoryTier::Dynamic => "DYNAMIC",
            };

            let sub_children = self.find_trie_children(edge.target_addr);
            let leaf_badge = if sub_children.is_empty() {
                " [LEAF/TERMINAL]"
            } else {
                ""
            };

            out.push_str(&format!(
                "  {}{}{} \"{}\" (0x{:04X}) | [{}] Bobot: {:.3} | Hit: {}x{}\n",
                indent,
                branch_sym,
                if sub_children.is_empty() {
                    " 🍂"
                } else {
                    " 🌿"
                },
                child_tok,
                edge.target_addr,
                tier_tag,
                edge.weight,
                edge.access_count,
                leaf_badge
            ));

            let next_indent = if is_last {
                format!("{}    ", indent)
            } else {
                format!("{}│   ", indent)
            };

            self.render_trie_branch_recursive(
                out,
                edge.target_addr,
                &next_indent,
                depth + 1,
                max_depth,
            );
        }
        if count > 4 {
            out.push_str(&format!(
                "  {}└── ... ({} cabang anak lainnya)\n",
                indent,
                count - 4
            ));
        }
    }
}
