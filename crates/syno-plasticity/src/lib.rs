//! Crate: syno-plasticity
//! Lapisan jaringan saraf tiruan plastis (SynapseNetwork), manajemen tingkat ingatan
//! (MemoryTier: Core vs Dynamic), dan penelusuran Hierarchical Trie.

pub mod network;
pub mod tier;
pub mod trie;

pub use network::{
    HebbianUpdateResult, OnlineLearningResult, Synapse, SynapseNetwork, WordTransitionLearningEvent,
};
pub use tier::MemoryTier;

#[cfg(test)]
mod tests {
    use super::*;
    use syno_vmm::VirtualMemory;

    #[test]
    fn test_hierarchical_trie_prefix_matching() {
        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        network.learn_sequence("selamat siang kawan.", 0.95, &mut vmm);
        network.learn_sequence("selamat malam kawan.", 0.95, &mut vmm);

        let match_siang = network.match_trie_prefix(&["selamat", "siang"]);
        let match_malam = network.match_trie_prefix(&["selamat", "malam"]);

        assert!(match_siang.is_some());
        assert!(match_malam.is_some());
        assert_ne!(match_siang, match_malam);

        let siang_addr = network.find_token_addr("siang").unwrap();
        let malam_addr = network.find_token_addr("malam").unwrap();

        assert_eq!(match_siang, Some(siang_addr));
        assert_eq!(match_malam, Some(malam_addr));
    }

    #[test]
    fn test_core_vs_dynamic_tier_priority() {
        let mut network = SynapseNetwork::new();
        let root = 0x1000;
        let core_child = 0x1020;
        let dynamic_child = 0x1040;

        network.connect_with_tier(root, dynamic_child, 0.90, MemoryTier::Dynamic);
        network.connect_with_tier(root, core_child, 0.95, MemoryTier::Core);

        let children = network.find_trie_children(root);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].target_addr, core_child);
        assert_eq!(children[0].tier, MemoryTier::Core);
        assert_eq!(children[1].target_addr, dynamic_child);
        assert_eq!(children[1].tier, MemoryTier::Dynamic);
    }

    #[test]
    fn test_fixed_weight_associative_memory_stability() {
        let mut network = SynapseNetwork::new();
        let id = network.connect_with_tier(0x1000, 0x1020, 0.95, MemoryTier::Core);

        let s1 = network.synapses.get(&id).unwrap();
        assert_eq!(s1.weight, 0.95);
        assert_eq!(s1.access_count, 1);

        let res1 = network.update_synapse(id).unwrap();
        assert_eq!(res1.new_weight, 0.95);
        assert_eq!(res1.access_count, 2);

        let res2 = network.update_synapse(id).unwrap();
        assert_eq!(res2.new_weight, 0.95);
        assert_eq!(res2.access_count, 3);
    }
}
