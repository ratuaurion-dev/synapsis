//! Module: decoder.rs
//! Modul Decoder biner ke teks (Binary-to-Text Decoder) untuk merekonstruksi raw bytes
//! dari VirtualMemory atau PersistentStorage menjadi kalimat teks UTF-8 yang bersih dan natural.

use syno_vmm::{PersistentStorage, VirtualMemory};

/// Hasil rekonstruksi decoding biner ke string teks
#[derive(Debug, Clone, Default)]
pub struct DecodedResult {
    /// Teks hasil decoding UTF-8 bersih
    pub text: String,
    /// Representasi heksadesimal dari raw bytes
    pub raw_hex: String,
    /// Jumlah total raw bytes awal yang diproses
    pub total_raw_bytes: usize,
    /// Jumlah byte teks bersih setelah mengabaikan padding/null
    pub payload_bytes: usize,
    /// Apakah seluruh payload merupakan karakter ASCII valid
    pub is_valid_ascii: bool,
}

impl DecodedResult {
    /// Format perbandingan transparan: Raw Bytes (Hex) -> Decoded String
    pub fn format_transformation(&self, max_hex_len: usize) -> String {
        let hex_display = if self.raw_hex.len() > max_hex_len {
            format!("{}...", &self.raw_hex[..max_hex_len])
        } else {
            self.raw_hex.clone()
        };
        format!("[{}] -> \"{}\"", hex_display, self.text)
    }
}

pub struct BinaryDecoder;

impl BinaryDecoder {
    /// Mendekode sekumpulan raw bytes biner menjadi string kalimat UTF-8 yang bersih.
    pub fn decode(bytes: &[u8]) -> DecodedResult {
        let total_raw_bytes = bytes.len();
        if bytes.is_empty() {
            return DecodedResult {
                text: String::from("(buffer biner kosong)"),
                raw_hex: String::new(),
                total_raw_bytes: 0,
                payload_bytes: 0,
                is_valid_ascii: true,
            };
        }

        let raw_hex: String = bytes
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<String>>()
            .join(" ");

        let mut start_idx = 0;
        while start_idx < bytes.len() {
            let b = bytes[start_idx];
            if (0x20..=0x7E).contains(&b) || (0xC2..=0xF4).contains(&b) {
                break;
            }
            start_idx += 1;
        }

        let mut clean_bytes = Vec::new();
        for &b in &bytes[start_idx..] {
            if b == 0x00 {
                break;
            }
            clean_bytes.push(b);
        }

        let payload_bytes = clean_bytes.len();
        let is_valid_ascii = clean_bytes.iter().all(|&b| b.is_ascii());

        let text = if clean_bytes.is_empty() {
            String::from("(pola biner korpus kosong)")
        } else {
            String::from_utf8_lossy(&clean_bytes).trim().to_string()
        };

        DecodedResult {
            text,
            raw_hex,
            total_raw_bytes,
            payload_bytes,
            is_valid_ascii,
        }
    }

    /// Membaca langsung dari VirtualMemory pada rentang alamat tertentu lalu mendekodenya.
    pub fn decode_from_memory(
        memory: &VirtualMemory,
        start_addr: usize,
        max_len: usize,
    ) -> DecodedResult {
        let bytes_at_0 = memory
            .peek_c_string(start_addr, max_len)
            .unwrap_or_default();
        let bytes = if !bytes_at_0.is_empty()
            && bytes_at_0
                .iter()
                .any(|&x| (0x20..=0x7E).contains(&x) || x >= 0xC0)
        {
            bytes_at_0
        } else {
            memory
                .peek_c_string(start_addr + 1, max_len)
                .unwrap_or_default()
        };

        Self::decode(&bytes)
    }

    /// Membaca langsung satu blok halaman memori dari persistent storage Redb lalu mendekodenya.
    pub fn decode_from_storage(
        storage: &PersistentStorage,
        page_addr: usize,
    ) -> Result<DecodedResult, String> {
        match storage
            .load_memory_block(page_addr)
            .map_err(|e| format!("{}", e))?
        {
            Some(block) => Ok(Self::decode(&block)),
            None => Ok(DecodedResult {
                text: String::from("(blok memori tidak ditemukan di database persisten)"),
                raw_hex: String::new(),
                total_raw_bytes: 0,
                payload_bytes: 0,
                is_valid_ascii: false,
            }),
        }
    }

    /// Format singkat transformasi decoding untuk keperluan logging
    pub fn format_transformation(bytes: &[u8]) -> String {
        let res = Self::decode(bytes);
        res.format_transformation(36)
    }
}
