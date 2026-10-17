//! Module: tier.rs
//! Kategori tingkat ketahanan memori (Memory Tier) dalam arsitektur neuroplastisitas Syno.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Tingkat keabadian ingatan dalam jaringan sinapsis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MemoryTier {
    /// Ingatan Inti: Berasal dari data latih / korpus dasar, bobot tetap 0.950, kebal peluruhan
    Core = 1,
    /// Ingatan Dinamis: Berasal dari stimulus interaktif otonom (>= 0x2000), bobot awal 0.650
    Dynamic = 2,
}

impl fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryTier::Core => write!(f, "Core"),
            MemoryTier::Dynamic => write!(f, "Dynamic"),
        }
    }
}

impl From<u8> for MemoryTier {
    fn from(val: u8) -> Self {
        match val {
            2 => MemoryTier::Dynamic,
            _ => MemoryTier::Core,
        }
    }
}

impl From<MemoryTier> for u8 {
    fn from(tier: MemoryTier) -> Self {
        match tier {
            MemoryTier::Core => 1,
            MemoryTier::Dynamic => 2,
        }
    }
}
