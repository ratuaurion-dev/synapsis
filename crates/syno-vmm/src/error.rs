//! Module: error.rs
//! Definisi tipe error untuk Virtual Memory Manager dan Persistent Storage redb.

use std::fmt;

/// Kesalahan operasi pada Virtual Memory Manager
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    OutOfBounds(usize),
    OutOfMemory,
    InvalidAddress(usize),
    AccessViolation(usize),
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryError::OutOfBounds(addr) => {
                write!(f, "Alamat 0x{:04X} melebihi batas memori", addr)
            }
            MemoryError::OutOfMemory => write!(f, "Memori fisik/halaman habis (Out of Memory)"),
            MemoryError::InvalidAddress(addr) => write!(f, "Alamat 0x{:04X} tidak valid", addr),
            MemoryError::AccessViolation(addr) => {
                write!(f, "Pelanggaran akses memori di 0x{:04X}", addr)
            }
        }
    }
}

impl std::error::Error for MemoryError {}

/// Kesalahan operasi pada Persistent Storage (redb)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageError(pub String);

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Storage error: {}", self.0)
    }
}

impl std::error::Error for StorageError {}

impl From<String> for StorageError {
    fn from(s: String) -> Self {
        StorageError(s)
    }
}

impl From<&str> for StorageError {
    fn from(s: &str) -> Self {
        StorageError(s.to_string())
    }
}
