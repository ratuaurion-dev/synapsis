//! Module: memory.rs
//! Custom Virtual Memory Manager (VMM) berbasis biner dengan dukungan Lazy Loading
//! dan penyimpanan persisten ACID menggunakan embedded database redb (O(1) RAM footprint).

use crate::error::{MemoryError, StorageError};
use crate::storage::{DynamicMemoryNode, PersistentStorage};

pub const PAGE_SIZE: usize = 64;
pub const NUM_PAGES: usize = 256;
pub const TOTAL_MEMORY: usize = PAGE_SIZE * NUM_PAGES; // 16384 bytes (16KB)

#[derive(Debug, Clone)]
pub struct Page {
    pub data: [u8; PAGE_SIZE],
    pub is_allocated: bool,
    pub is_loaded: bool,
    pub access_count: u32,
    pub is_dirty: bool,
}

impl Page {
    pub fn new() -> Self {
        Self {
            data: [0; PAGE_SIZE],
            is_allocated: false,
            is_loaded: false,
            access_count: 0,
            is_dirty: false,
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VirtualMemory {
    pub pages: Vec<Page>,
    pub storage: Option<PersistentStorage>,
    pub dynamic_nodes_cache: Vec<DynamicMemoryNode>,
}

impl VirtualMemory {
    /// Inisialisasi Virtual Memory Manager standar tanpa database persisten.
    pub fn new() -> Self {
        let mut pages = Vec::with_capacity(NUM_PAGES);
        for _ in 0..NUM_PAGES {
            pages.push(Page::new());
        }
        Self {
            pages,
            storage: None,
            dynamic_nodes_cache: Vec::new(),
        }
    }

    /// Inisialisasi Virtual Memory Manager dengan dukungan redb persistent storage.
    /// Menerapkan mekanisme LAZY LOADING: hanya memindai alamat halaman yang sudah ada
    /// tanpa memuat isi blok datanya sekaligus ke RAM (O(1) RAM consumption).
    pub fn with_storage(storage: PersistentStorage) -> Result<Self, StorageError> {
        let mut vmm = Self::new();
        let persisted_addrs = storage.list_persisted_page_addresses()?;
        for addr in persisted_addrs {
            let page_idx = addr / PAGE_SIZE;
            if page_idx < NUM_PAGES {
                vmm.pages[page_idx].is_allocated = true;
                vmm.pages[page_idx].is_loaded = false; // Lazy Loading: data belum dimuat ke RAM
            }
        }
        if let Ok(Some(nodes)) = storage.load_dynamic_nodes() {
            vmm.dynamic_nodes_cache = nodes;
        }
        vmm.storage = Some(storage);
        Ok(vmm)
    }

    /// Menghubungkan storage persisten ke instance VirtualMemory yang sudah ada.
    pub fn attach_storage(&mut self, storage: PersistentStorage) -> Result<(), StorageError> {
        let persisted_addrs = storage.list_persisted_page_addresses()?;
        for addr in persisted_addrs {
            let page_idx = addr / PAGE_SIZE;
            if page_idx < NUM_PAGES {
                self.pages[page_idx].is_allocated = true;
                if !self.pages[page_idx].is_dirty {
                    self.pages[page_idx].is_loaded = false;
                }
            }
        }
        if let Ok(Some(nodes)) = storage.load_dynamic_nodes() {
            self.dynamic_nodes_cache = nodes;
        }
        self.storage = Some(storage);
        Ok(())
    }

    /// Memastikan halaman termuat di RAM secara on-demand (Demand Paging / Lazy Loading).
    pub fn ensure_page_loaded(&mut self, page_idx: usize) -> Result<(), MemoryError> {
        if page_idx >= NUM_PAGES {
            return Err(MemoryError::OutOfBounds(page_idx * PAGE_SIZE));
        }
        let page = &mut self.pages[page_idx];
        if !page.is_loaded {
            if let Some(ref storage) = self.storage {
                let page_addr = page_idx * PAGE_SIZE;
                if let Ok(Some(bytes)) = storage.load_memory_block(page_addr) {
                    let copy_len = bytes.len().min(PAGE_SIZE);
                    page.data[..copy_len].copy_from_slice(&bytes[..copy_len]);
                    page.is_allocated = true;
                }
            }
            page.is_loaded = true;
        }
        Ok(())
    }

    /// Menghitung jumlah halaman yang saat ini aktif residen di RAM fisik.
    pub fn resident_pages_count(&self) -> usize {
        self.pages.iter().filter(|p| p.is_loaded).count()
    }

    /// Menghitung total alokasi memori yang terdaftar (baik di RAM maupun di database).
    pub fn allocated_pages_count(&self) -> usize {
        self.pages.iter().filter(|p| p.is_allocated).count()
    }

    /// Mengosongkan halaman dari RAM jika diperlukan untuk menjaga konsumsi RAM O(1).
    pub fn unload_page(&mut self, page_idx: usize) {
        if page_idx < NUM_PAGES && self.storage.is_some() {
            let page = &mut self.pages[page_idx];
            if page.is_dirty {
                if let Some(ref storage) = self.storage {
                    let page_addr = page_idx * PAGE_SIZE;
                    let _ = storage.save_memory_block(page_addr, &page.data);
                }
            }
            page.data.fill(0);
            page.is_loaded = false;
        }
    }

    /// Menyimpan seluruh halaman yang berstatus dirty ke database redb dalam satu transaksi batch ACID
    pub fn flush_dirty_pages_batch(&mut self) -> Result<usize, StorageError> {
        if let Some(ref storage) = self.storage {
            let mut dirty_blocks: Vec<(usize, &[u8])> = Vec::new();
            for (i, page) in self.pages.iter().enumerate() {
                if page.is_dirty {
                    let page_addr = i * PAGE_SIZE;
                    dirty_blocks.push((page_addr, &page.data));
                }
            }
            if !dirty_blocks.is_empty() {
                let count = dirty_blocks.len();
                storage.save_memory_blocks_batch(&dirty_blocks)?;
                for page in &mut self.pages {
                    page.is_dirty = false;
                }
                return Ok(count);
            }
        }
        Ok(0)
    }

    /// Alokasikan rentang memori berurutan (first-fit page allocation)
    pub fn allocate(&mut self, size: usize) -> Result<usize, MemoryError> {
        if size == 0 {
            return Ok(0);
        }
        let pages_needed = size.div_ceil(PAGE_SIZE);

        let mut contiguous_count = 0;
        let mut start_page = None;

        for (i, page) in self.pages.iter().enumerate() {
            if !page.is_allocated {
                if contiguous_count == 0 {
                    start_page = Some(i);
                }
                contiguous_count += 1;
                if contiguous_count == pages_needed {
                    break;
                }
            } else {
                contiguous_count = 0;
                start_page = None;
            }
        }

        if contiguous_count < pages_needed || start_page.is_none() {
            return Err(MemoryError::OutOfMemory);
        }

        let start = start_page.unwrap();
        for i in start..(start + pages_needed) {
            self.pages[i].is_allocated = true;
            self.pages[i].is_loaded = true;
            self.pages[i].data.fill(0);
            self.pages[i].access_count = 0;
            self.pages[i].is_dirty = false;

            // Sinkronisasi alokasi baru ke database redb secara ACID
            if let Some(ref storage) = self.storage {
                let page_addr = i * PAGE_SIZE;
                let _ = storage.save_memory_block(page_addr, &self.pages[i].data);
            }
        }

        Ok(start * PAGE_SIZE)
    }

    /// Dealokasi halaman pada rentang alamat tertentu
    pub fn deallocate(&mut self, addr: usize, size: usize) -> Result<(), MemoryError> {
        if addr >= TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(addr));
        }
        let start_page = addr / PAGE_SIZE;
        let pages_count = size.div_ceil(PAGE_SIZE);

        for i in start_page..(start_page + pages_count).min(NUM_PAGES) {
            self.pages[i].is_allocated = false;
            self.pages[i].is_loaded = false;
            self.pages[i].data.fill(0);
            self.pages[i].access_count = 0;
            self.pages[i].is_dirty = false;
            if let Some(ref storage) = self.storage {
                let page_addr = i * PAGE_SIZE;
                let _ = storage.save_memory_block(page_addr, &self.pages[i].data);
            }
        }
        Ok(())
    }

    /// Tulis 1 byte ke alamat virtual dan sinkronkan ke database redb
    pub fn write_byte(&mut self, addr: usize, byte: u8) -> Result<(), MemoryError> {
        if addr >= TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(addr));
        }
        let page_idx = addr / PAGE_SIZE;
        let offset = addr % PAGE_SIZE;

        self.ensure_page_loaded(page_idx)?;

        let page = &mut self.pages[page_idx];
        page.data[offset] = byte;
        page.is_dirty = true;
        page.is_allocated = true;
        page.access_count = page.access_count.saturating_add(1);

        // Sinkronisasi pembaruan memori ke redb secara langsung (ACID transaction)
        if let Some(ref storage) = self.storage {
            let page_addr = page_idx * PAGE_SIZE;
            let _ = storage.save_memory_block(page_addr, &page.data);
        }

        Ok(())
    }

    /// Baca 1 byte dari alamat virtual (memuat halaman secara on-demand jika belum di RAM)
    pub fn read_byte(&mut self, addr: usize) -> Result<u8, MemoryError> {
        if addr >= TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(addr));
        }
        let page_idx = addr / PAGE_SIZE;
        let offset = addr % PAGE_SIZE;

        self.ensure_page_loaded(page_idx)?;

        let page = &mut self.pages[page_idx];
        page.access_count = page.access_count.saturating_add(1);
        Ok(page.data[offset])
    }

    /// Tulis sekumpulan raw bytes ke alamat virtual dengan sinkronisasi ACID ke redb
    pub fn write_bytes(&mut self, start_addr: usize, data: &[u8]) -> Result<(), MemoryError> {
        if start_addr + data.len() > TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(start_addr + data.len()));
        }

        let mut curr_page = usize::MAX;
        for (i, &b) in data.iter().enumerate() {
            let addr = start_addr + i;
            let page_idx = addr / PAGE_SIZE;
            let offset = addr % PAGE_SIZE;

            if page_idx != curr_page {
                self.ensure_page_loaded(page_idx)?;
                curr_page = page_idx;
            }

            let page = &mut self.pages[page_idx];
            page.data[offset] = b;
            page.is_dirty = true;
            page.is_allocated = true;
            page.access_count = page.access_count.saturating_add(1);
        }

        // Sinkronkan seluruh halaman yang terpengaruh ke redb dalam transaksi ACID
        if let Some(ref storage) = self.storage {
            let start_page = start_addr / PAGE_SIZE;
            let end_page = (start_addr + data.len()).div_ceil(PAGE_SIZE);
            for p in start_page..end_page {
                let page_addr = p * PAGE_SIZE;
                let _ = storage.save_memory_block(page_addr, &self.pages[p].data);
            }
        }

        Ok(())
    }

    /// Baca sekumpulan raw bytes dari alamat virtual
    pub fn read_bytes(&mut self, start_addr: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        if start_addr + len > TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(start_addr + len));
        }
        let mut buffer = Vec::with_capacity(len);
        for i in 0..len {
            buffer.push(self.read_byte(start_addr + i)?);
        }
        Ok(buffer)
    }

    /// Membaca 1 byte secara pasif/konstan (read-only) dengan on-demand read dari storage jika belum ada di RAM (O(1) RAM)
    pub fn peek_byte(&self, addr: usize) -> Result<u8, MemoryError> {
        if addr >= TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(addr));
        }
        let page_idx = addr / PAGE_SIZE;
        let offset = addr % PAGE_SIZE;

        let page = &self.pages[page_idx];
        if page.is_loaded {
            return Ok(page.data[offset]);
        }

        // Lazy Loading on-demand read O(1) RAM langsung dari embedded storage redb
        if let Some(ref storage) = self.storage {
            let page_addr = page_idx * PAGE_SIZE;
            if let Ok(Some(bytes)) = storage.load_memory_block(page_addr) {
                if offset < bytes.len() {
                    return Ok(bytes[offset]);
                }
            }
        }

        Ok(0)
    }

    /// Membaca rentang byte secara pasif/konstan (read-only) dengan on-demand read (O(1) RAM)
    pub fn peek_bytes(&self, start_addr: usize, len: usize) -> Result<Vec<u8>, MemoryError> {
        if start_addr + len > TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(start_addr + len));
        }
        let mut buffer = Vec::with_capacity(len);
        for i in 0..len {
            buffer.push(self.peek_byte(start_addr + i)?);
        }
        Ok(buffer)
    }

    /// Mengisi rentang memori dengan nilai byte nol (zeroing)
    pub fn zero_range(&mut self, start_addr: usize, len: usize) -> Result<(), MemoryError> {
        if start_addr + len > TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(start_addr + len));
        }
        let zeros = vec![0u8; len];
        self.write_bytes(start_addr, &zeros)
    }

    /// Membaca byte dari memori virtual hingga mendeteksi null-terminator (0x00) atau batas max_len
    pub fn peek_c_string(&self, start_addr: usize, max_len: usize) -> Result<Vec<u8>, MemoryError> {
        let mut buffer = Vec::new();
        let limit = (start_addr + max_len).min(TOTAL_MEMORY);
        for addr in start_addr..limit {
            let b = self.peek_byte(addr)?;
            if b == 0x00 {
                break;
            }
            buffer.push(b);
        }
        Ok(buffer)
    }

    /// Menulis bytes teks dengan pembersihan padding zero-byte (0x00) di sekeliling blok
    pub fn write_string_clean(
        &mut self,
        addr: usize,
        text: &[u8],
        block_size: usize,
    ) -> Result<(), MemoryError> {
        if addr + block_size > TOTAL_MEMORY {
            return Err(MemoryError::OutOfBounds(addr + block_size));
        }
        let write_len = text.len().min(block_size.saturating_sub(1));
        self.write_bytes(addr, &text[..write_len])?;
        if block_size > write_len {
            self.zero_range(addr + write_len, block_size - write_len)?;
        }
        Ok(())
    }

    /// Menghasilkan representasi hexdump dari blok memori untuk inspeksi
    pub fn hexdump(&self, start_addr: usize, len: usize) -> String {
        let mut output = String::new();
        let end_addr = (start_addr + len).min(TOTAL_MEMORY);
        let mut curr = start_addr;

        while curr < end_addr {
            let chunk_len = (16).min(end_addr - curr);

            output.push_str(&format!("  [0x{:04X}] ", curr));
            let mut ascii = String::new();

            for i in 0..16 {
                if i < chunk_len {
                    let b = self.peek_byte(curr + i).unwrap_or(0);
                    output.push_str(&format!("{:02X} ", b));
                    if b.is_ascii_graphic() || b == b' ' {
                        ascii.push(b as char);
                    } else {
                        ascii.push('.');
                    }
                } else {
                    output.push_str("   ");
                }
            }
            output.push_str(&format!("| {} |\n", ascii));
            curr += chunk_len;
        }

        output
    }
}

impl Default for VirtualMemory {
    fn default() -> Self {
        Self::new()
    }
}
