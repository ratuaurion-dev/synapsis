//! Crate: syno-compiler
//! Lapisan mesin bitcode, kompailer DSL, eksekusi opcode, dan binary-to-text decoder.

pub mod compiler;
pub mod decoder;

pub use compiler::{MiniCompiler, Opcode};
pub use decoder::{BinaryDecoder, DecodedResult};
