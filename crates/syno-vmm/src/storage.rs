//! Module: storage.rs
//! Modul penyimpanan persisten berbasis embedded database murni Rust (redb)
//! dengan pendekatan lazy loading / on-demand read untuk konsumsi RAM O(1).

use crate::error::StorageError;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MEMORY_PAGES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("memory_pages");
pub const SYNAPSES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("synapses");
pub const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");

#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub is_new: bool,
    pub persisted_memory_blocks: usize,
    pub persisted_synapses: usize,
    pub next_synapse_id: usize,
}

/// Struktur simpul memori dinamis otonom yang diserap ke zona Dynamic (>= 0x2000)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DynamicMemoryNode {
    pub target_addr: usize,
    pub keywords: Vec<String>,
    pub raw_sentence: String,
}

/// Representasi serialisasi rekaman sinapsis pada level storage (decoupled dari SynapseNetwork)
#[derive(Debug, Clone, PartialEq)]
pub struct SynapseRecord {
    pub id: usize,
    pub source_addr: usize,
    pub target_addr: usize,
    pub weight: f32,
    pub access_count: u32,
    pub is_active: bool,
    pub tier: u8,
}

#[derive(Clone)]
pub struct PersistentStorage {
    db: Arc<Database>,
    pub db_path: PathBuf,
}

impl PersistentStorage {
    /// Inisialisasi database redb: buka jika sudah ada, buat baru jika belum
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<(Self, StorageStats), StorageError> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| StorageError(format!("Gagal membuat direktori DB: {}", e)))?;
        }

        let is_new = !db_path.exists();

        let db = if !is_new {
            Database::open(&db_path)
                .map_err(|e| StorageError(format!("Gagal membuka redb: {}", e)))?
        } else {
            Database::create(&db_path)
                .map_err(|e| StorageError(format!("Gagal membuat redb: {}", e)))?
        };

        // Pastikan seluruh tabel terdaftar dalam skema database
        let write_txn = db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal memulai write txn: {}", e)))?;
        {
            let _ = write_txn
                .open_table(MEMORY_PAGES_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table memory_pages: {}", e)))?;
            let _ = write_txn
                .open_table(SYNAPSES_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table synapses: {}", e)))?;
            let _ = write_txn
                .open_table(METADATA_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table metadata: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit inisialisasi tabel: {}", e)))?;

        let storage = Self {
            db: Arc::new(db),
            db_path,
        };

        let mut stats = storage.get_stats()?;
        stats.is_new = is_new;

        Ok((storage, stats))
    }

    /// Menyimpan satu blok memori virtual (misal 64 bytes) ke database secara ACID
    pub fn save_memory_block(&self, addr: usize, data: &[u8]) -> Result<(), StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal memulai write txn memory: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(MEMORY_PAGES_TABLE)
                .map_err(|e| StorageError(format!("Gagal membuka tabel memory_pages: {}", e)))?;
            table.insert(addr as u64, data).map_err(|e| {
                StorageError(format!(
                    "Gagal insert memory block di 0x{:04X}: {}",
                    addr, e
                ))
            })?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit memory block: {}", e)))?;
        Ok(())
    }

    /// Menyimpan sekumpulan blok memori dalam satu transaksi batch ACID redb
    pub fn save_memory_blocks_batch(&self, blocks: &[(usize, &[u8])]) -> Result<(), StorageError> {
        if blocks.is_empty() {
            return Ok(());
        }
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal memulai write txn memory batch: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(MEMORY_PAGES_TABLE)
                .map_err(|e| StorageError(format!("Gagal membuka tabel memory_pages: {}", e)))?;
            for &(addr, data) in blocks {
                table.insert(addr as u64, data).map_err(|e| {
                    StorageError(format!(
                        "Gagal insert memory block di 0x{:04X}: {}",
                        addr, e
                    ))
                })?;
            }
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit memory blocks batch: {}", e)))?;
        Ok(())
    }

    /// Membaca satu blok memori secara on-demand / lazy loading dari database
    pub fn load_memory_block(&self, addr: usize) -> Result<Option<Vec<u8>>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal memulai read txn memory: {}", e)))?;
        let table = read_txn
            .open_table(MEMORY_PAGES_TABLE)
            .map_err(|e| StorageError(format!("Gagal membuka tabel memory_pages: {}", e)))?;
        let result = table
            .get(addr as u64)
            .map_err(|e| StorageError(format!("Gagal get memory block 0x{:04X}: {}", addr, e)))?;
        Ok(result.map(|val| val.value().to_vec()))
    }

    /// Mengambil daftar alamat halaman memori yang tersimpan di database tanpa memuat isi bloknya
    pub fn list_persisted_page_addresses(&self) -> Result<Vec<usize>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal read txn list memory: {}", e)))?;
        let table = read_txn
            .open_table(MEMORY_PAGES_TABLE)
            .map_err(|e| StorageError(format!("Gagal buka table memory_pages: {}", e)))?;
        let mut addrs = Vec::new();
        let iter = table
            .iter()
            .map_err(|e| StorageError(format!("Gagal iterasi memory: {}", e)))?;
        for item in iter {
            let (key, _val) =
                item.map_err(|e| StorageError(format!("Gagal baca key memory: {}", e)))?;
            addrs.push(key.value() as usize);
        }
        Ok(addrs)
    }

    /// Menyimpan sinapsis tunggal ke database secara ACID
    pub fn save_synapse_record(&self, record: &SynapseRecord) -> Result<(), StorageError> {
        let payload = Self::serialize_synapse_record(record);
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal memulai write txn synapse: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(SYNAPSES_TABLE)
                .map_err(|e| StorageError(format!("Gagal membuka tabel synapses: {}", e)))?;
            table
                .insert(record.id as u64, payload.as_slice())
                .map_err(|e| StorageError(format!("Gagal insert synapse #{}: {}", record.id, e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit synapse #{}: {}", record.id, e)))?;
        Ok(())
    }

    /// Menyimpan sekumpulan sinapsis dalam satu transaksi batch ACID
    pub fn save_synapse_records_batch(
        &self,
        records: &[SynapseRecord],
    ) -> Result<(), StorageError> {
        if records.is_empty() {
            return Ok(());
        }
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal write txn synapses batch: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(SYNAPSES_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table synapses: {}", e)))?;
            for s in records {
                let payload = Self::serialize_synapse_record(s);
                table
                    .insert(s.id as u64, payload.as_slice())
                    .map_err(|e| StorageError(format!("Gagal insert synapse #{}: {}", s.id, e)))?;
            }
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit synapses batch: {}", e)))?;
        Ok(())
    }

    /// Membaca sinapsis tunggal secara on-demand dari database
    pub fn load_synapse_record(&self, id: usize) -> Result<Option<SynapseRecord>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal memulai read txn synapse: {}", e)))?;
        let table = read_txn
            .open_table(SYNAPSES_TABLE)
            .map_err(|e| StorageError(format!("Gagal membuka tabel synapses: {}", e)))?;
        let result = table
            .get(id as u64)
            .map_err(|e| StorageError(format!("Gagal get synapse #{}: {}", id, e)))?;
        Ok(result.and_then(|val| Self::deserialize_synapse_record(val.value())))
    }

    /// Membaca seluruh sinapsis yang tersimpan sebelumnya untuk rekonsolidasi topologi jaringan
    pub fn load_all_synapse_records(&self) -> Result<Vec<SynapseRecord>, StorageError> {
        let read_txn = self.db.begin_read().map_err(|e| {
            StorageError(format!("Gagal memulai read txn load all synapses: {}", e))
        })?;
        let table = read_txn
            .open_table(SYNAPSES_TABLE)
            .map_err(|e| StorageError(format!("Gagal membuka tabel synapses: {}", e)))?;

        let mut synapses = Vec::new();
        let iter = table
            .iter()
            .map_err(|e| StorageError(format!("Gagal iterasi synapses: {}", e)))?;
        for item in iter {
            let (_key, val) =
                item.map_err(|e| StorageError(format!("Gagal baca item synapse: {}", e)))?;
            if let Some(s) = Self::deserialize_synapse_record(val.value()) {
                synapses.push(s);
            }
        }
        Ok(synapses)
    }

    /// Menghapus sinapsis tunggal dari tabel sinapsis redb secara ACID
    pub fn delete_synapse(&self, id: usize) -> Result<bool, StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal write txn delete synapse: {}", e)))?;
        let removed = {
            let mut table = write_txn
                .open_table(SYNAPSES_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table synapses: {}", e)))?;
            let prev = table
                .remove(id as u64)
                .map_err(|e| StorageError(format!("Gagal hapus synapse #{}: {}", id, e)))?;
            prev.is_some()
        };
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit delete synapse: {}", e)))?;
        Ok(removed)
    }

    /// Menghapus sekumpulan sinapsis dalam satu transaksi batch ACID redb
    pub fn delete_synapses_batch(&self, ids: &[usize]) -> Result<usize, StorageError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal write txn delete synapses batch: {}", e)))?;
        let mut count = 0;
        {
            let mut table = write_txn
                .open_table(SYNAPSES_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table synapses: {}", e)))?;
            for &id in ids {
                if table
                    .remove(id as u64)
                    .map_err(|e| StorageError(format!("Gagal hapus synapse #{}: {}", id, e)))?
                    .is_some()
                {
                    count += 1;
                }
            }
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit delete synapses batch: {}", e)))?;
        Ok(count)
    }

    /// Menyimpan nilai next_synapse_id ke metadata
    pub fn save_next_synapse_id(&self, next_id: usize) -> Result<(), StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal write txn metadata: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(METADATA_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table metadata: {}", e)))?;
            let bytes = (next_id as u64).to_be_bytes();
            table
                .insert("next_synapse_id", bytes.as_slice())
                .map_err(|e| StorageError(format!("Gagal insert metadata: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit metadata: {}", e)))?;
        Ok(())
    }

    /// Membaca nilai next_synapse_id dari metadata
    pub fn load_next_synapse_id(&self) -> Result<Option<usize>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal read txn metadata: {}", e)))?;
        let table = read_txn
            .open_table(METADATA_TABLE)
            .map_err(|e| StorageError(format!("Gagal buka table metadata: {}", e)))?;
        let result = table
            .get("next_synapse_id")
            .map_err(|e| StorageError(format!("Gagal get metadata: {}", e)))?;

        Ok(result.and_then(|val| {
            let slice = val.value();
            if slice.len() == 8 {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(slice);
                Some(u64::from_be_bytes(arr) as usize)
            } else {
                None
            }
        }))
    }

    /// Menyimpan nilai biner ke tabel metadata secara ACID
    pub fn save_metadata(&self, key: &str, data: &[u8]) -> Result<(), StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError(format!("Gagal write txn metadata: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(METADATA_TABLE)
                .map_err(|e| StorageError(format!("Gagal buka table metadata: {}", e)))?;
            table
                .insert(key, data)
                .map_err(|e| StorageError(format!("Gagal insert metadata '{}': {}", key, e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError(format!("Gagal commit metadata: {}", e)))?;
        Ok(())
    }

    /// Membaca nilai biner dari tabel metadata secara lazy loading
    pub fn load_metadata(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal read txn metadata: {}", e)))?;
        let table = read_txn
            .open_table(METADATA_TABLE)
            .map_err(|e| StorageError(format!("Gagal buka table metadata: {}", e)))?;
        let result = table
            .get(key)
            .map_err(|e| StorageError(format!("Gagal get metadata '{}': {}", key, e)))?;
        Ok(result.map(|val| val.value().to_vec()))
    }

    /// Menyimpan registry token (Word -> Virtual Address) ke metadata database secara ACID
    pub fn save_token_registry(
        &self,
        registry: &HashMap<String, usize>,
    ) -> Result<(), StorageError> {
        let json = serde_json::to_vec(registry)
            .map_err(|e| StorageError(format!("Gagal serialisasi token registry: {}", e)))?;
        self.save_metadata("token_registry", &json)
    }

    /// Memuat registry token (Word -> Virtual Address) dari metadata database
    pub fn load_token_registry(&self) -> Result<Option<HashMap<String, usize>>, StorageError> {
        match self.load_metadata("token_registry")? {
            Some(bytes) => {
                let map = serde_json::from_slice(&bytes).map_err(|e| {
                    StorageError(format!("Gagal deserialisasi token registry: {}", e))
                })?;
                Ok(Some(map))
            }
            None => Ok(None),
        }
    }

    /// Menyimpan daftar alamat simpul akar pohon percabangan (Trie roots) ke metadata database
    pub fn save_trie_roots(&self, roots: &[usize]) -> Result<(), StorageError> {
        let json = serde_json::to_vec(roots)
            .map_err(|e| StorageError(format!("Gagal serialisasi trie roots: {}", e)))?;
        self.save_metadata("trie_roots", &json)
    }

    /// Memuat daftar alamat simpul akar pohon percabangan (Trie roots) dari metadata database
    pub fn load_trie_roots(&self) -> Result<Option<Vec<usize>>, StorageError> {
        match self.load_metadata("trie_roots")? {
            Some(bytes) => {
                let roots = serde_json::from_slice(&bytes)
                    .map_err(|e| StorageError(format!("Gagal deserialisasi trie roots: {}", e)))?;
                Ok(Some(roots))
            }
            None => Ok(None),
        }
    }

    /// Menyimpan daftar simpul memori dinamis (DynamicMemoryNode) ke metadata redb
    pub fn save_dynamic_nodes(&self, nodes: &[DynamicMemoryNode]) -> Result<(), StorageError> {
        let json = serde_json::to_vec(nodes)
            .map_err(|e| StorageError(format!("Gagal serialisasi dynamic nodes: {}", e)))?;
        self.save_metadata("dynamic_nodes", &json)
    }

    /// Memuat daftar simpul memori dinamis (DynamicMemoryNode) dari metadata redb
    pub fn load_dynamic_nodes(&self) -> Result<Option<Vec<DynamicMemoryNode>>, StorageError> {
        match self.load_metadata("dynamic_nodes")? {
            Some(bytes) => {
                let nodes = serde_json::from_slice(&bytes).map_err(|e| {
                    StorageError(format!("Gagal deserialisasi dynamic nodes: {}", e))
                })?;
                Ok(Some(nodes))
            }
            None => Ok(None),
        }
    }

    /// Mengambil metrik statistik penyimpanan persisten redb
    pub fn get_stats(&self) -> Result<StorageStats, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError(format!("Gagal read txn get stats: {}", e)))?;

        let mem_table = read_txn
            .open_table(MEMORY_PAGES_TABLE)
            .map_err(|e| StorageError(format!("Gagal buka memory_pages table: {}", e)))?;
        let persisted_memory_blocks = mem_table
            .len()
            .map_err(|e| StorageError(format!("Gagal hitung memory blocks: {}", e)))?
            as usize;

        let syn_table = read_txn
            .open_table(SYNAPSES_TABLE)
            .map_err(|e| StorageError(format!("Gagal buka synapses table: {}", e)))?;
        let persisted_synapses = syn_table
            .len()
            .map_err(|e| StorageError(format!("Gagal hitung synapses: {}", e)))?
            as usize;

        let next_synapse_id = self.load_next_synapse_id()?.unwrap_or(1);

        Ok(StorageStats {
            is_new: false,
            persisted_memory_blocks,
            persisted_synapses,
            next_synapse_id,
        })
    }

    /// Serialisasi biner efisien untuk struktur SynapseRecord (34 bytes: id, src, dst, weight, access, active, tier)
    pub fn serialize_synapse_record(s: &SynapseRecord) -> Vec<u8> {
        let mut buf = Vec::with_capacity(34);
        buf.extend_from_slice(&(s.id as u64).to_be_bytes());
        buf.extend_from_slice(&(s.source_addr as u64).to_be_bytes());
        buf.extend_from_slice(&(s.target_addr as u64).to_be_bytes());
        buf.extend_from_slice(&s.weight.to_be_bytes());
        buf.extend_from_slice(&s.access_count.to_be_bytes());
        buf.push(if s.is_active { 1 } else { 0 });
        buf.push(s.tier);
        buf
    }

    /// Deserialisasi biner ke struktur SynapseRecord
    pub fn deserialize_synapse_record(bytes: &[u8]) -> Option<SynapseRecord> {
        if bytes.len() < 33 {
            return None;
        }

        let mut id_bytes = [0u8; 8];
        id_bytes.copy_from_slice(&bytes[0..8]);
        let id = u64::from_be_bytes(id_bytes) as usize;

        let mut src_bytes = [0u8; 8];
        src_bytes.copy_from_slice(&bytes[8..16]);
        let source_addr = u64::from_be_bytes(src_bytes) as usize;

        let mut dst_bytes = [0u8; 8];
        dst_bytes.copy_from_slice(&bytes[16..24]);
        let target_addr = u64::from_be_bytes(dst_bytes) as usize;

        let mut weight_bytes = [0u8; 4];
        weight_bytes.copy_from_slice(&bytes[24..28]);
        let weight = f32::from_be_bytes(weight_bytes);

        let mut access_bytes = [0u8; 4];
        access_bytes.copy_from_slice(&bytes[28..32]);
        let access_count = u32::from_be_bytes(access_bytes);

        let is_active = bytes[32] != 0;
        let tier = if bytes.len() >= 34 { bytes[33] } else { 1 };

        Some(SynapseRecord {
            id,
            source_addr,
            target_addr,
            weight,
            access_count,
            is_active,
            tier,
        })
    }
}
