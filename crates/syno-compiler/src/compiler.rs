//! Module: compiler.rs
//! Modul kompailer mini untuk menerjemahkan teks Assembly plastis (DSL) menjadi representasi bitcode biner.

pub use crate::decoder::{BinaryDecoder, DecodedResult};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Halt = 0x00,
    Alloc = 0x01,
    Write = 0x02,
    Read = 0x03,
    Synapse = 0x04,
    Pulse = 0x05,
    Rewire = 0x06,
    Prune = 0x07,
    Chaos = 0x08,
}

impl Opcode {
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Opcode::Halt),
            0x01 => Some(Opcode::Alloc),
            0x02 => Some(Opcode::Write),
            0x03 => Some(Opcode::Read),
            0x04 => Some(Opcode::Synapse),
            0x05 => Some(Opcode::Pulse),
            0x06 => Some(Opcode::Rewire),
            0x07 => Some(Opcode::Prune),
            0x08 => Some(Opcode::Chaos),
            _ => None,
        }
    }
}

pub struct MiniCompiler;

impl MiniCompiler {
    /// Mengompilasi string teks instruksi menjadi sekumpulan bitcode biner
    pub fn compile(source: &str) -> Result<Vec<u8>, String> {
        let mut bytecode = Vec::new();

        for (line_no, line) in source.lines().enumerate() {
            let line = line.trim();
            // Lewati baris kosong atau komentar
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();
            let command = tokens[0].to_uppercase();

            match command.as_str() {
                "HALT" => {
                    bytecode.push(Opcode::Halt as u8);
                }
                "ALLOC" => {
                    if tokens.len() < 2 {
                        return Err(format!("Baris {}: ALLOC butuh argumen <size>", line_no + 1));
                    }
                    let size = Self::parse_u16(tokens[1])?;
                    bytecode.push(Opcode::Alloc as u8);
                    bytecode.extend_from_slice(&size.to_be_bytes());
                }
                "WRITE" => {
                    if tokens.len() < 3 {
                        return Err(format!(
                            "Baris {}: WRITE butuh argumen <addr> <byte1> [byte2...]",
                            line_no + 1
                        ));
                    }
                    let addr = Self::parse_u16(tokens[1])?;
                    let mut data = Vec::new();
                    for token in &tokens[2..] {
                        let byte_val = Self::parse_u8(token)?;
                        data.push(byte_val);
                    }
                    if data.len() > 255 {
                        return Err(format!(
                            "Baris {}: WRITE data maksimal 255 bytes",
                            line_no + 1
                        ));
                    }
                    bytecode.push(Opcode::Write as u8);
                    bytecode.extend_from_slice(&addr.to_be_bytes());
                    bytecode.push(data.len() as u8);
                    bytecode.extend_from_slice(&data);
                }
                "READ" => {
                    if tokens.len() < 3 {
                        return Err(format!("Baris {}: READ butuh <addr> <len>", line_no + 1));
                    }
                    let addr = Self::parse_u16(tokens[1])?;
                    let len = Self::parse_u8(tokens[2])?;
                    bytecode.push(Opcode::Read as u8);
                    bytecode.extend_from_slice(&addr.to_be_bytes());
                    bytecode.push(len);
                }
                "SYNAPSE" => {
                    if tokens.len() < 4 {
                        return Err(format!(
                            "Baris {}: SYNAPSE butuh <src_addr> <dst_addr> <weight_pct 0-100>",
                            line_no + 1
                        ));
                    }
                    let src = Self::parse_u16(tokens[1])?;
                    let dst = Self::parse_u16(tokens[2])?;
                    let weight = Self::parse_u8(tokens[3])?;
                    bytecode.push(Opcode::Synapse as u8);
                    bytecode.extend_from_slice(&src.to_be_bytes());
                    bytecode.extend_from_slice(&dst.to_be_bytes());
                    bytecode.push(weight.min(100));
                }
                "PULSE" => {
                    if tokens.len() < 2 {
                        return Err(format!("Baris {}: PULSE butuh <synapse_id>", line_no + 1));
                    }
                    let id = Self::parse_u16(tokens[1])?;
                    bytecode.push(Opcode::Pulse as u8);
                    bytecode.extend_from_slice(&id.to_be_bytes());
                }
                "REWIRE" => {
                    if tokens.len() < 3 {
                        return Err(format!(
                            "Baris {}: REWIRE butuh <synapse_id> <new_dst_addr>",
                            line_no + 1
                        ));
                    }
                    let id = Self::parse_u16(tokens[1])?;
                    let new_dst = Self::parse_u16(tokens[2])?;
                    bytecode.push(Opcode::Rewire as u8);
                    bytecode.extend_from_slice(&id.to_be_bytes());
                    bytecode.extend_from_slice(&new_dst.to_be_bytes());
                }
                "PRUNE" => {
                    bytecode.push(Opcode::Prune as u8);
                }
                "CHAOS" => {
                    if tokens.len() < 3 {
                        return Err(format!("Baris {}: CHAOS butuh <addr> <len>", line_no + 1));
                    }
                    let addr = Self::parse_u16(tokens[1])?;
                    let len = Self::parse_u8(tokens[2])?;
                    bytecode.push(Opcode::Chaos as u8);
                    bytecode.extend_from_slice(&addr.to_be_bytes());
                    bytecode.push(len);
                }
                unknown => {
                    return Err(format!(
                        "Baris {}: Instruksi tidak dikenal '{}'",
                        line_no + 1,
                        unknown
                    ));
                }
            }
        }

        Ok(bytecode)
    }

    /// Disassembler untuk membaca balik representasi biner menjadi string terbaca
    pub fn disassemble(bytecode: &[u8]) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        let mut idx = 0;

        while idx < bytecode.len() {
            let op_byte = bytecode[idx];
            idx += 1;

            match Opcode::from_u8(op_byte) {
                Some(Opcode::Halt) => result.push("HALT".to_string()),
                Some(Opcode::Alloc) => {
                    if idx + 2 > bytecode.len() {
                        return Err("Bytecode terpotong pada ALLOC".to_string());
                    }
                    let size = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    idx += 2;
                    result.push(format!("ALLOC {}", size));
                }
                Some(Opcode::Write) => {
                    if idx + 3 > bytecode.len() {
                        return Err("Bytecode terpotong pada header WRITE".to_string());
                    }
                    let addr = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    let len = bytecode[idx + 2] as usize;
                    idx += 3;
                    if idx + len > bytecode.len() {
                        return Err("Bytecode terpotong pada payload WRITE".to_string());
                    }
                    let data: Vec<String> = bytecode[idx..idx + len]
                        .iter()
                        .map(|b| format!("0x{:02X}", b))
                        .collect();
                    idx += len;
                    result.push(format!("WRITE 0x{:04X} [{}]", addr, data.join(" ")));
                }
                Some(Opcode::Read) => {
                    if idx + 3 > bytecode.len() {
                        return Err("Bytecode terpotong pada READ".to_string());
                    }
                    let addr = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    let len = bytecode[idx + 2];
                    idx += 3;
                    result.push(format!("READ 0x{:04X} len:{}", addr, len));
                }
                Some(Opcode::Synapse) => {
                    if idx + 5 > bytecode.len() {
                        return Err("Bytecode terpotong pada SYNAPSE".to_string());
                    }
                    let src = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    let dst = u16::from_be_bytes([bytecode[idx + 2], bytecode[idx + 3]]);
                    let weight = bytecode[idx + 4];
                    idx += 5;
                    result.push(format!(
                        "SYNAPSE 0x{:04X} -> 0x{:04X} ({}%)",
                        src, dst, weight
                    ));
                }
                Some(Opcode::Pulse) => {
                    if idx + 2 > bytecode.len() {
                        return Err("Bytecode terpotong pada PULSE".to_string());
                    }
                    let id = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    idx += 2;
                    result.push(format!("PULSE #{}", id));
                }
                Some(Opcode::Rewire) => {
                    if idx + 4 > bytecode.len() {
                        return Err("Bytecode terpotong pada REWIRE".to_string());
                    }
                    let id = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    let new_dst = u16::from_be_bytes([bytecode[idx + 2], bytecode[idx + 3]]);
                    idx += 4;
                    result.push(format!("REWIRE #{} -> 0x{:04X}", id, new_dst));
                }
                Some(Opcode::Prune) => result.push("PRUNE".to_string()),
                Some(Opcode::Chaos) => {
                    if idx + 3 > bytecode.len() {
                        return Err("Bytecode terpotong pada CHAOS".to_string());
                    }
                    let addr = u16::from_be_bytes([bytecode[idx], bytecode[idx + 1]]);
                    let len = bytecode[idx + 2];
                    idx += 3;
                    result.push(format!("CHAOS 0x{:04X} len:{}", addr, len));
                }
                None => return Err(format!("Opcode biner tidak valid: 0x{:02X}", op_byte)),
            }
        }

        Ok(result)
    }

    /// Mengalirkan potongan teks korpus (chunk) menjadi bitcode biner secara bertahap.
    /// Potongan teks dikonversi menjadi instruksi WRITE ke memori virtual,
    /// dan asosiasi sekuensial antar blok teks dihubungkan dengan SYNAPSE.
    pub fn compile_text_chunk(text: &str, base_addr: usize) -> Result<Vec<u8>, String> {
        let mut bytecode = Vec::new();
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Ok(bytecode);
        }

        // Tulis bytes teks ke alamat memori dalam potongan maksimal 32 byte per blok
        let mut offset = 0;
        let mut prev_token_addr: Option<usize> = None;

        for chunk in bytes.chunks(32) {
            let addr = (base_addr + offset) as u16;
            bytecode.push(Opcode::Write as u8);
            bytecode.extend_from_slice(&addr.to_be_bytes());
            bytecode.push(chunk.len() as u8);
            bytecode.extend_from_slice(chunk);

            // Jika ada alamat blok sebelumnya, hubungkan dengan sinapsis asosiatif
            if let Some(prev) = prev_token_addr {
                bytecode.push(Opcode::Synapse as u8);
                bytecode.extend_from_slice(&(prev as u16).to_be_bytes());
                bytecode.extend_from_slice(&addr.to_be_bytes());
                bytecode.push(50); // Bobot awal 50%
            }

            prev_token_addr = Some(base_addr + offset);
            offset += chunk.len();
        }

        // Tulis null-terminator (0x00) di akhir payload teks untuk mencegah string bleeding
        let term_addr = (base_addr + offset) as u16;
        bytecode.push(Opcode::Write as u8);
        bytecode.extend_from_slice(&term_addr.to_be_bytes());
        bytecode.push(1);
        bytecode.push(0x00);

        Ok(bytecode)
    }

    /// Mengompilasi sebuah kalimat teks menjadi bitcode biner dan payload raw bytes (null-terminated)
    /// yang siap ditulis langsung ke blok VirtualMemory.
    pub fn compile_sentence_to_block(
        sentence: &str,
        base_addr: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let clean = sentence.trim();
        let bytecode = Self::compile_text_chunk(clean, base_addr)?;
        let mut raw_bytes = clean.as_bytes().to_vec();
        raw_bytes.push(0x00); // Pastikan null-terminator di akhir string
        Ok((bytecode, raw_bytes))
    }

    fn parse_u16(token: &str) -> Result<u16, String> {
        if let Some(stripped) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            u16::from_str_radix(stripped, 16)
                .map_err(|_| format!("Format hexadesimal u16 tidak valid: {}", token))
        } else {
            token
                .parse::<u16>()
                .map_err(|_| format!("Format desimal u16 tidak valid: {}", token))
        }
    }

    fn parse_u8(token: &str) -> Result<u8, String> {
        if let Some(stripped) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            u8::from_str_radix(stripped, 16)
                .map_err(|_| format!("Format hexadesimal u8 tidak valid: {}", token))
        } else {
            token
                .parse::<u8>()
                .map_err(|_| format!("Format desimal u8 tidak valid: {}", token))
        }
    }

    /// Menjalankan instruksi opcode Virtual Memory dasar (Halt, Alloc, Write, Read) pada VirtualMemory
    pub fn execute_vmm_opcodes(
        bytecode: &[u8],
        vmm: &mut syno_vmm::VirtualMemory,
    ) -> Result<usize, String> {
        let mut pc = 0;
        while pc < bytecode.len() {
            let op = bytecode[pc];
            pc += 1;
            match Opcode::from_u8(op) {
                Some(Opcode::Halt) => break,
                Some(Opcode::Alloc) => {
                    if pc + 2 > bytecode.len() {
                        return Err("Unexpected EOF on Alloc opcode".to_string());
                    }
                    let size = u16::from_be_bytes([bytecode[pc], bytecode[pc + 1]]) as usize;
                    pc += 2;
                    vmm.allocate(size).map_err(|e| e.to_string())?;
                }
                Some(Opcode::Write) => {
                    if pc + 3 > bytecode.len() {
                        return Err("Unexpected EOF on Write opcode".to_string());
                    }
                    let addr = u16::from_be_bytes([bytecode[pc], bytecode[pc + 1]]) as usize;
                    let len = bytecode[pc + 2] as usize;
                    pc += 3;
                    if pc + len > bytecode.len() {
                        return Err("Unexpected EOF on Write payload".to_string());
                    }
                    let data = &bytecode[pc..pc + len];
                    pc += len;
                    vmm.write_bytes(addr, data).map_err(|e| e.to_string())?;
                }
                Some(Opcode::Read) => {
                    if pc + 3 > bytecode.len() {
                        return Err("Unexpected EOF on Read opcode".to_string());
                    }
                    let addr = u16::from_be_bytes([bytecode[pc], bytecode[pc + 1]]) as usize;
                    let len = bytecode[pc + 2] as usize;
                    pc += 3;
                    vmm.read_bytes(addr, len).map_err(|e| e.to_string())?;
                }
                Some(Opcode::Synapse) => {
                    if pc + 5 > bytecode.len() {
                        return Err("Unexpected EOF on Synapse opcode".to_string());
                    }
                    pc += 5;
                }
                Some(Opcode::Pulse) => {
                    if pc + 2 > bytecode.len() {
                        return Err("Unexpected EOF on Pulse opcode".to_string());
                    }
                    pc += 2;
                }
                Some(Opcode::Rewire) => {
                    if pc + 4 > bytecode.len() {
                        return Err("Unexpected EOF on Rewire opcode".to_string());
                    }
                    pc += 4;
                }
                Some(Opcode::Prune) => {}
                Some(Opcode::Chaos) => {
                    if pc + 3 > bytecode.len() {
                        return Err("Unexpected EOF on Chaos opcode".to_string());
                    }
                    pc += 3;
                }
                None => return Err(format!("Unknown opcode 0x{:02X} at PC {}", op, pc - 1)),
            }
        }
        Ok(pc)
    }
}
