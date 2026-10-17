//! Module: reasoning.rs
//! Modul logika penalaran (reasoning) dan pembacaan pola asosiatif sinapsis.
//! Beroperasi secara murni pada referensi immutabel memori virtual dan jaringan sinapsis
//! serta menggunakan BinaryDecoder untuk menerjemahkan raw bytes biner menjadi teks UTF-8 natural.

use crate::engine::{VerbalEngine, VerbalInferenceResult as AutoregressiveGeneration};
use std::collections::HashSet;
use syno_compiler::BinaryDecoder;
use syno_plasticity::SynapseNetwork;
use syno_vmm::VirtualMemory;

pub type TreeReasoningEngine<'a> = ReasoningEngine<'a>;

/// Kategori simpul respons dalam arsitektur memori asosiatif
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResponseCategory {
    ConversationalGreeting,
    ConversationalMorning,
    ConversationalSalam,
    ConversationalStatus,
    ConversationalNight,
    ConversationalAfternoon,
    ConversationalIdentity,
    ConversationalCapability,
    ConversationalHelp,
    ConversationalTech,
    ConversationalThanks,
    ConversationalFarewell,
    ConversationalPreference,
    ConversationalKnowledge,
    ConversationalOrigin,
    ConversationalLocation,
    ConversationalEmotion,
    ConversationalHobby,
    ConversationalWisdom,
    LiteratureCorpusFallback,
    #[serde(other)]
    General,
}

/// Struktur simpul memori respons terstruktur
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResponseMemoryNode {
    pub target_addr: usize,
    pub category: ResponseCategory,
    pub description: String,
    pub keywords: Vec<String>,
}

/// Representasi jalur sinapsis asosiatif beserta cuplikan data biner pada memori target
#[derive(Debug, Clone)]
pub struct AssociationPath {
    pub source_addr: usize,
    pub target_addr: usize,
    pub weight: f32,
    pub data_sample: Vec<u8>,
    pub decoded_preview: String,
}

/// Hasil evaluasi penalaran pola memori
#[derive(Debug, Clone)]
pub struct ReasoningTrace {
    pub query_addr: usize,
    pub best_target_addr: usize,
    pub is_conversational: bool,
    pub pathways: Vec<AssociationPath>,
    pub cumulative_confidence: f32,
    pub resolved_bytes: Vec<u8>,
    pub raw_hex: String,
    pub decoded_text: String,
}

/// Mesin penalaran berbasis pola sinapsis
pub struct ReasoningEngine<'a> {
    memory: &'a VirtualMemory,
    network: &'a SynapseNetwork,
}

impl<'a> ReasoningEngine<'a> {
    /// Inisialisasi ReasoningEngine dengan referensi immutabel ke VMM dan Jaringan Sinapsis
    pub fn new(memory: &'a VirtualMemory, network: &'a SynapseNetwork) -> Self {
        Self { memory, network }
    }

    /// Menelusuri seluruh jalur sinapsis aktif yang terhubung dari node awal (query_addr)
    pub fn trace_pathways(
        &self,
        query_addr: usize,
        threshold: f32,
        max_depth: usize,
        sample_len: usize,
    ) -> Vec<AssociationPath> {
        let mut pathways = Vec::new();
        let mut visited = HashSet::new();
        let mut frontier = vec![(query_addr, 1.0f32, 0usize)];

        while let Some((curr_addr, acc_weight, depth)) = frontier.pop() {
            if depth >= max_depth || visited.contains(&curr_addr) {
                continue;
            }
            visited.insert(curr_addr);

            for synapse in self.network.find_outgoing_transitions(curr_addr) {
                if synapse.is_active && synapse.weight >= threshold {
                    let path_weight = acc_weight * synapse.weight;
                    let data_sample = self
                        .memory
                        .peek_bytes(synapse.target_addr, sample_len)
                        .unwrap_or_default();
                    let decoded_preview = BinaryDecoder::decode(&data_sample).text;

                    pathways.push(AssociationPath {
                        source_addr: synapse.source_addr,
                        target_addr: synapse.target_addr,
                        weight: path_weight,
                        data_sample,
                        decoded_preview,
                    });

                    frontier.push((synapse.target_addr, path_weight, depth + 1));
                }
            }
        }

        pathways.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        pathways
    }

    /// Mengevaluasi penalaran asosiatif dari query_addr
    pub fn evaluate_associative_reasoning(
        &self,
        query_addr: usize,
        threshold: f32,
        max_depth: usize,
        sample_len: usize,
    ) -> Option<ReasoningTrace> {
        let pathways = self.trace_pathways(query_addr, threshold, max_depth, sample_len);
        if pathways.is_empty() {
            return None;
        }

        let best = &pathways[0];
        let cumulative_confidence =
            pathways.iter().map(|p| p.weight).sum::<f32>() / (pathways.len() as f32);
        let resolved_bytes = self
            .memory
            .peek_bytes(best.target_addr, sample_len)
            .unwrap_or_default();
        let decoded = BinaryDecoder::decode(&resolved_bytes);

        Some(ReasoningTrace {
            query_addr,
            best_target_addr: best.target_addr,
            is_conversational: (0x0200..=0x0F00).contains(&best.target_addr),
            pathways,
            cumulative_confidence,
            resolved_bytes,
            raw_hex: decoded.raw_hex,
            decoded_text: decoded.text,
        })
    }

    /// Mengambil seluruh simpul memori respons terstruktur yang terdaftar
    pub fn list_registered_response_nodes(&self) -> Vec<ResponseMemoryNode> {
        let mut nodes = Vec::new();
        if let Ok(entries) =
            crate::loader::ConversationDataLoader::load_or_create_default("data/conversations.json")
        {
            for entry in entries {
                nodes.push(entry.to_memory_node());
            }
        }
        nodes
    }

    /// Auto-Regressive Generation Loop Berbasis Pohon Percabangan (Tree-Based Reasoning Engine):
    /// Didelegasikan secara langsung ke modul terpusat `VerbalEngine`.
    pub fn generate_response_autoregressive(
        &self,
        seed_token: &str,
        max_length: usize,
    ) -> Option<AutoregressiveGeneration> {
        VerbalEngine::generate_response_readonly(self.memory, self.network, seed_token, max_length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::VerbalStopReason as GenerationStopReason;
    use syno_plasticity::SynapseNetwork;
    use syno_vmm::VirtualMemory;

    #[test]
    fn test_tree_reasoning_terminal_stop() {
        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        network.learn_sequence("kabar baik sistem stabil.", 0.95, &mut vmm);

        let reasoning = ReasoningEngine::new(&vmm, &network);
        let gen = reasoning
            .generate_response_autoregressive("kabar", 20)
            .unwrap();

        assert_eq!(gen.stop_reason, GenerationStopReason::TerminalPunctuation);
        assert!(gen.composite_sentence.contains("stabil."));
    }

    #[test]
    fn test_tree_reasoning_anti_looping_cycle_prevention() {
        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        // Bentuk siklus A -> B -> A
        network.learn_sequence("halo kawan halo", 0.90, &mut vmm);

        let reasoning = ReasoningEngine::new(&vmm, &network);
        let gen = reasoning
            .generate_response_autoregressive("halo", 20)
            .unwrap();

        // Harus dicegah dari perulangan siklus
        assert!(gen.steps.len() <= 3);
        assert!(matches!(
            gen.stop_reason,
            GenerationStopReason::CycleDetected | GenerationStopReason::NoOutgoingTransition
        ));
    }
}
