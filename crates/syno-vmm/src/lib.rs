//! Crate: syno-vmm
//! Lapisan fondasi Virtual Memory Manager, alokasi halaman O(1),
//! dan penyimpanan persisten ACID berbasis embedded database redb.

pub mod error;
pub mod memory;
pub mod storage;

pub use error::{MemoryError, StorageError};
pub use memory::{Page, VirtualMemory, NUM_PAGES, PAGE_SIZE, TOTAL_MEMORY};
pub use storage::{
    DynamicMemoryNode, PersistentStorage, StorageStats, SynapseRecord, MEMORY_PAGES_TABLE,
    METADATA_TABLE, SYNAPSES_TABLE,
};
