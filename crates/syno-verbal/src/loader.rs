//! Module: loader.rs
//! Modul data loader efisien untuk mengunduh, memotong korpus teks,
//! dan mengalirkan (streaming) teks ke bitcode biner dan pohon percabangan Trie.

use crate::reasoning::{ResponseCategory, ResponseMemoryNode};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use syno_compiler::MiniCompiler;
use syno_plasticity::SynapseNetwork;
use syno_vmm::{VirtualMemory, TOTAL_MEMORY};

/// Konfigurasi untuk TextDataLoader
#[derive(Debug, Clone)]
pub struct DataLoaderConfig {
    pub max_words: usize,
    pub batch_words: usize,
    pub source_url: String,
    pub local_path: PathBuf,
    pub base_memory_addr: usize,
}

impl Default for DataLoaderConfig {
    fn default() -> Self {
        Self {
            max_words: 1_000_000,
            batch_words: 80,
            source_url: "https://raw.githubusercontent.com/karpathy/char-rnn/master/data/tinyshakespeare/input.txt".to_string(),
            local_path: PathBuf::from("data/tinyshakespeare.txt"),
            base_memory_addr: 0x0000,
        }
    }
}

/// Metrik dan statistik dari proses streaming data loader
#[derive(Debug, Clone, Default)]
pub struct LoaderStats {
    pub words_processed: usize,
    pub bytes_read: usize,
    pub batches_streamed: usize,
    pub bytecode_bytes_generated: usize,
}

pub struct TextDataLoader {
    pub config: DataLoaderConfig,
}

impl TextDataLoader {
    pub fn new(config: DataLoaderConfig) -> Self {
        Self { config }
    }

    pub fn ensure_dataset(&self) -> Result<PathBuf, String> {
        let path = &self.config.local_path;

        if path.exists() {
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.len() > 0 {
                    return Ok(path.clone());
                }
            }
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Gagal membuat direktori {:?}: {}", parent, e))?;
        }

        println!(
            "  [*] Mengunduh korpus teks dari: {}",
            self.config.source_url
        );

        let curl_status = Command::new("curl")
            .arg("-sSL")
            .arg("--max-time")
            .arg("15")
            .arg(&self.config.source_url)
            .arg("-o")
            .arg(path)
            .status();

        let download_success = match curl_status {
            Ok(status) => {
                status.success()
                    && path.exists()
                    && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
            }
            Err(_) => false,
        };

        if download_success {
            println!("  [+] Korpus berhasil diunduh ke {:?}", path);
            Ok(path.clone())
        } else {
            println!("  [!] Unduhan gagal/offline. Menggunakan korpus teks cadangan lokal.");
            self.write_fallback_corpus(path)?;
            Ok(path.clone())
        }
    }

    fn write_fallback_corpus(&self, path: &Path) -> Result<(), String> {
        let sample = "\
First Citizen:
Before we proceed any further, hear me speak.

All:
Speak, speak.

First Citizen:
You are all resolved rather to die than to famish?

All:
Resolved. resolved.

First Citizen:
First, you know Caius Marcius is chief enemy to the people.

All:
We know't, we know't.

First Citizen:
Let us kill him, and we'll have corn at our own price.
Is't a verdict?

All:
No more talking on't; let it be done: away, away!
";
        let mut file =
            File::create(path).map_err(|e| format!("Gagal membuat file fallback: {}", e))?;
        for _ in 0..100 {
            file.write_all(sample.as_bytes())
                .map_err(|e| format!("Gagal menulis data fallback: {}", e))?;
        }
        Ok(())
    }

    pub fn stream_to_compiler<F>(&self, mut on_batch: F) -> Result<LoaderStats, String>
    where
        F: FnMut(usize, &str, &[u8]) -> Result<(), String>,
    {
        let dataset_path = self.ensure_dataset()?;
        let file = File::open(&dataset_path)
            .map_err(|e| format!("Gagal membuka file korpus {:?}: {}", dataset_path, e))?;

        let mut reader = BufReader::with_capacity(64 * 1024, file);
        let mut stats = LoaderStats::default();
        let mut line_buffer = String::with_capacity(512);
        let mut current_words: Vec<String> = Vec::with_capacity(self.config.batch_words);
        let mut batch_index = 0;

        loop {
            line_buffer.clear();
            let bytes_read = reader
                .read_line(&mut line_buffer)
                .map_err(|e| format!("Gagal membaca stream teks: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            stats.bytes_read += bytes_read;

            for word in line_buffer.split_whitespace() {
                if stats.words_processed >= self.config.max_words {
                    break;
                }

                current_words.push(word.to_string());
                stats.words_processed += 1;

                if current_words.len() >= self.config.batch_words {
                    batch_index += 1;
                    let batch_text = current_words.join(" ");
                    let addr = self.config.base_memory_addr + ((batch_index * 32) % 512);

                    let bytecode = MiniCompiler::compile_text_chunk(&batch_text, addr)?;
                    stats.bytecode_bytes_generated += bytecode.len();
                    stats.batches_streamed += 1;

                    on_batch(batch_index, &batch_text, &bytecode)?;

                    current_words.clear();
                }
            }

            if stats.words_processed >= self.config.max_words {
                break;
            }
        }

        if !current_words.is_empty() {
            batch_index += 1;
            let batch_text = current_words.join(" ");
            let addr = self.config.base_memory_addr + ((batch_index * 32) % 512);

            let bytecode = MiniCompiler::compile_text_chunk(&batch_text, addr)?;
            stats.bytecode_bytes_generated += bytecode.len();
            stats.batches_streamed += 1;

            on_batch(batch_index, &batch_text, &bytecode)?;
        }

        Ok(stats)
    }
}

/// Entri satu respons percakapan yang dimuat dari file eksternal (JSON)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationEntry {
    pub target_addr: String,
    pub category: ResponseCategory,
    pub description: String,
    pub keywords: Vec<String>,
    pub response: String,
}

impl ConversationEntry {
    pub fn parse_addr(&self) -> usize {
        if let Some(stripped) = self
            .target_addr
            .strip_prefix("0x")
            .or_else(|| self.target_addr.strip_prefix("0X"))
        {
            usize::from_str_radix(stripped, 16).unwrap_or(0x0200)
        } else {
            self.target_addr.parse::<usize>().unwrap_or(0x0200)
        }
    }

    pub fn to_memory_node(&self) -> ResponseMemoryNode {
        ResponseMemoryNode {
            target_addr: self.parse_addr(),
            category: self.category,
            description: self.description.clone(),
            keywords: self.keywords.clone(),
        }
    }
}

pub struct ConversationDataLoader;

impl ConversationDataLoader {
    pub fn load_or_create_default<P: AsRef<Path>>(
        path: P,
    ) -> Result<Vec<ConversationEntry>, String> {
        let p = path.as_ref();
        if !p.exists() {
            let default_entries = Self::default_entries();
            let json = serde_json::to_string_pretty(&default_entries)
                .map_err(|e| format!("Gagal serialisasi default conversations: {}", e))?;
            if let Some(parent) = p.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(p, json).map_err(|e| format!("Gagal menulis {}: {}", p.display(), e))?;
            return Ok(default_entries);
        }

        let content =
            fs::read_to_string(p).map_err(|e| format!("Gagal membaca {}: {}", p.display(), e))?;
        let entries: Vec<ConversationEntry> = serde_json::from_str(&content)
            .map_err(|e| format!("Gagal parse JSON {}: {}", p.display(), e))?;
        Ok(entries)
    }

    pub fn default_entries() -> Vec<ConversationEntry> {
        vec![
            ConversationEntry {
                target_addr: "0x0200".to_string(),
                category: ResponseCategory::ConversationalGreeting,
                description: "Sapaan Percakapan (Halo/Hi)".to_string(),
                keywords: vec![
                    "halo".into(),
                    "hi".into(),
                    "hai".into(),
                    "hello".into(),
                    "hey".into(),
                    "hei".into(),
                ],
                response: "Halo juga! Jaringan sinapsis aktif merespons sapaan Anda.".to_string(),
            },
            ConversationEntry {
                target_addr: "0x0280".to_string(),
                category: ResponseCategory::ConversationalMorning,
                description: "Sapaan Pagi (Morning/Good)".to_string(),
                keywords: vec!["pagi".into(), "morning".into(), "good".into()],
                response: "Pagi yang cerah, mari kita bereksperimen dengan komputasi plastis."
                    .to_string(),
            },
            ConversationEntry {
                target_addr: "0x0300".to_string(),
                category: ResponseCategory::ConversationalSalam,
                description: "Salam & Keakraban".to_string(),
                keywords: vec![
                    "salam".into(),
                    "assalam".into(),
                    "peace".into(),
                    "berkah".into(),
                    "rahmat".into(),
                ],
                response: "Salam hangat! Node memori neuroplastis terhubung prima.".to_string(),
            },
            ConversationEntry {
                target_addr: "0x0380".to_string(),
                category: ResponseCategory::ConversationalStatus,
                description: "Kabar & Status Sistem".to_string(),
                keywords: vec![
                    "kabar".into(),
                    "status".into(),
                    "bagaimana".into(),
                    "kondisi".into(),
                    "sehat".into(),
                    "prima".into(),
                ],
                response: "Kabar baik, sistem biner dan alokasi memori berjalan stabil."
                    .to_string(),
            },
            ConversationEntry {
                target_addr: "0x0400".to_string(),
                category: ResponseCategory::ConversationalNight,
                description: "Sapaan Malam".to_string(),
                keywords: vec![
                    "malam".into(),
                    "night".into(),
                    "petang".into(),
                    "tidur".into(),
                ],
                response: "Selamat malam! Proses konsolidasi memori dan sinapsis siap mendampingi."
                    .to_string(),
            },
            ConversationEntry {
                target_addr: "0x0480".to_string(),
                category: ResponseCategory::ConversationalAfternoon,
                description: "Sapaan Siang / Sore".to_string(),
                keywords: vec!["siang".into(), "sore".into(), "afternoon".into()],
                response: "Selamat siang! Seluruh jalur komputasi virtual beroperasi optimal."
                    .to_string(),
            },
        ]
    }
}

/// Metrik dan statistik dari proses pemuatan dataset terbuka secara massal
#[derive(Debug, Clone, Default)]
pub struct BulkIngestionStats {
    pub dataset_source: String,
    pub total_lines: usize,
    pub total_sentences: usize,
    pub total_bytecode_bytes: usize,
    pub total_memory_blocks: usize,
    pub new_synapses_count: usize,
    pub reinforced_synapses_count: usize,
    pub total_tokens_registered: usize,
}

pub struct BulkDatasetLoader;

impl BulkDatasetLoader {
    pub fn ensure_default_corpus<P: AsRef<Path>>(path: P) -> Result<PathBuf, String> {
        let p = path.as_ref();
        if p.exists() {
            if let Ok(metadata) = fs::metadata(p) {
                if metadata.len() > 0 {
                    return Ok(p.to_path_buf());
                }
            }
        }

        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Gagal membuat direktori {:?}: {}", parent, e))?;
        }

        let default_content = "\
# Dataset Korpus Terbuka Synapsis (Pemuatan Massal / Bulk Ingestion)
# Kalimat-kalimat domain komputasi kognitif, neuroplastisitas biner, dan interaksi cerdas

Kecerdasan buatan berbasis neuroplastisitas biner memproses informasi secara adaptif.
Jaringan sinapsis menstimulasi memori virtual secara non-linier dan efisien.
Penyimpanan persisten embedded database menjaga konsumsi memori RAM tetap konstan O(1).
Auto-regressive generation loop merangkai kalimat kata demi kata secara otonom.
Pembelajaran Hebbian memperkuat jalur koneksi yang sering diakses pengguna.
Sistem komputasi plastis merespons masukan teks dengan kecepatan tinggi.
Alokasi memori virtual dipetakan ke dalam blok-blok biner persisten.
Perturbasi non-linier dan chaos engine menjaga dinamika jaringan neural.
Transformasi biner ke teks mendekode raw bytes menjadi string kalimat yang koheren.
Interaksi percakapan manusia dan mesin berjalan interaktif secara real-time.
Transisi kata n-gram mencatat hubungan asosiatif antar token bahasa.
Arsitektur neuroplastis memadukan bitcode virtual machine dan pembelajaran mesin.
Pengurangan bobot sinapsis secara proporsional menjaga keseimbangan topologi.
Kompailer mini menerjemahkan teks instruksi menjadi representasi biner.
Format biner murni menjamin efisiensi alokasi tanpa overhead berlebih.
";
        fs::write(p, default_content)
            .map_err(|e| format!("Gagal menulis dataset default ke {}: {}", p.display(), e))?;
        Ok(p.to_path_buf())
    }

    pub fn stream_sentences<P, F>(path: P, mut on_sentence: F) -> Result<usize, String>
    where
        P: AsRef<Path>,
        F: FnMut(usize, &str) -> Result<(), String>,
    {
        let p = path.as_ref();
        if !p.exists() {
            return Err(format!("File dataset tidak ditemukan: {}", p.display()));
        }

        let extension = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut count = 0;

        if extension.eq_ignore_ascii_case("json") {
            let content = fs::read_to_string(p)
                .map_err(|e| format!("Gagal membaca JSON {}: {}", p.display(), e))?;

            if let Ok(strings) = serde_json::from_str::<Vec<String>>(&content) {
                for s in strings {
                    let clean = s.trim();
                    if !clean.is_empty() {
                        count += 1;
                        on_sentence(count, clean)?;
                    }
                }
            } else if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                for val in values {
                    let sentence_opt = val
                        .get("sentence")
                        .or_else(|| val.get("text"))
                        .or_else(|| val.get("response"))
                        .and_then(|v| v.as_str());

                    if let Some(s) = sentence_opt {
                        let clean = s.trim();
                        if !clean.is_empty() {
                            count += 1;
                            on_sentence(count, clean)?;
                        }
                    }
                }
            } else {
                return Err(format!(
                    "Format JSON pada {} tidak dikenali sebagai array kalimat/objek",
                    p.display()
                ));
            }
        } else {
            let file =
                File::open(p).map_err(|e| format!("Gagal membuka file {}: {}", p.display(), e))?;
            let mut reader = BufReader::with_capacity(64 * 1024, file);
            let mut line_buffer = String::with_capacity(512);

            loop {
                line_buffer.clear();
                let bytes = reader
                    .read_line(&mut line_buffer)
                    .map_err(|e| format!("Gagal membaca baris dataset: {}", e))?;
                if bytes == 0 {
                    break;
                }
                let clean = line_buffer.trim();
                if clean.is_empty() || clean.starts_with('#') || clean.starts_with("//") {
                    continue;
                }
                count += 1;
                on_sentence(count, clean)?;
            }
        }

        Ok(count)
    }
}

pub struct BulkBinaryIngestionEngine;

impl BulkBinaryIngestionEngine {
    pub fn ingest_dataset<P: AsRef<Path>>(
        dataset_path: P,
        base_memory_addr: usize,
        initial_weight: f32,
        memory: &mut VirtualMemory,
        network: &mut SynapseNetwork,
    ) -> Result<BulkIngestionStats, String> {
        let path = dataset_path.as_ref();
        let path_str = path.display().to_string();

        let mut stats = BulkIngestionStats {
            dataset_source: path_str,
            ..Default::default()
        };

        let mut sentence_batch = Vec::new();
        let mut slot_offset = 0;

        BulkDatasetLoader::stream_sentences(path, |_idx, sentence| {
            stats.total_lines += 1;
            stats.total_sentences += 1;

            let slot_addr = base_memory_addr + (slot_offset % 0x0900);
            slot_offset += 64;

            let (bytecode, raw_bytes) =
                MiniCompiler::compile_sentence_to_block(sentence, slot_addr)?;
            stats.total_bytecode_bytes += bytecode.len();

            if slot_addr + raw_bytes.len() <= TOTAL_MEMORY {
                let _ = memory.zero_range(slot_addr, 64);
                let write_len = raw_bytes.len().min(63);
                let _ = memory.write_bytes(slot_addr, &raw_bytes[..write_len]);
                let _ = memory.write_byte(slot_addr + write_len, 0x00);
                stats.total_memory_blocks += 1;
            }

            sentence_batch.push(sentence.to_string());

            if sentence_batch.len() >= 25 {
                let (new_syn, reinforced) =
                    network.bulk_learn_sentences(&sentence_batch, initial_weight, memory);
                stats.new_synapses_count += new_syn;
                stats.reinforced_synapses_count += reinforced;
                sentence_batch.clear();
            }

            Ok(())
        })?;

        if !sentence_batch.is_empty() {
            let (new_syn, reinforced) =
                network.bulk_learn_sentences(&sentence_batch, initial_weight, memory);
            stats.new_synapses_count += new_syn;
            stats.reinforced_synapses_count += reinforced;
            sentence_batch.clear();
        }

        let _ = memory.flush_dirty_pages_batch();

        stats.total_tokens_registered = network.token_to_addr.len();
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_default_corpus_and_stream() {
        let temp_dir = std::env::temp_dir().join("synapsis_test_bulk");
        let file_path = temp_dir.join("test_corpus.txt");
        let _ = fs::remove_file(&file_path);

        let created_path = BulkDatasetLoader::ensure_default_corpus(&file_path).unwrap();
        assert!(created_path.exists());

        let mut lines = Vec::new();
        let total = BulkDatasetLoader::stream_sentences(&file_path, |_idx, line| {
            lines.push(line.to_string());
            Ok(())
        })
        .unwrap();

        assert_eq!(total, lines.len());
        assert!(total >= 10);
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_stream_json_sentences() {
        let temp_dir = std::env::temp_dir().join("synapsis_test_bulk_json");
        let file_path = temp_dir.join("test_corpus.json");
        let _ = fs::create_dir_all(&temp_dir);

        let json_content = r#"[
            "Kecerdasan buatan biner memproses data secara neuroplastis.",
            "Jaringan sinapsis menghubungkan memori secara teratur."
        ]"#;
        fs::write(&file_path, json_content).unwrap();

        let mut sentences = Vec::new();
        let total = BulkDatasetLoader::stream_sentences(&file_path, |_idx, s| {
            sentences.push(s.to_string());
            Ok(())
        })
        .unwrap();

        assert_eq!(total, 2);
        assert_eq!(
            sentences[0],
            "Kecerdasan buatan biner memproses data secara neuroplastis."
        );
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_bulk_binary_ingestion_execution() {
        let temp_dir = std::env::temp_dir().join("synapsis_test_ingestion");
        let file_path = temp_dir.join("mini_corpus.txt");
        let _ = fs::create_dir_all(&temp_dir);

        let content = "sistem biner sangat cepat.\nmemori virtual persisten stabil.\n";
        fs::write(&file_path, content).unwrap();

        let mut vmm = VirtualMemory::new();
        let mut network = SynapseNetwork::new();

        let stats = BulkBinaryIngestionEngine::ingest_dataset(
            &file_path,
            0x0600,
            0.65,
            &mut vmm,
            &mut network,
        )
        .unwrap();

        assert_eq!(stats.total_sentences, 2);
        assert!(stats.total_bytecode_bytes > 0);
        assert_eq!(stats.total_memory_blocks, 2);
        assert!(stats.new_synapses_count > 0);
        assert!(stats.total_tokens_registered >= 6);

        let _ = fs::remove_file(&file_path);
    }
}
