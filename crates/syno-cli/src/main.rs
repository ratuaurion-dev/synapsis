//! Crate: syno-cli (Lapis 6 - Antarmuka)
//!
//! Antarmuka baris perintah (CLI) untuk Synapsis Virtual Machine.
//! Hanya bertugas mengurai argumen CLI (`--train`, `--chat`, `--help`) dan
//! memanggil API publik dari crate-crate di bawahnya tanpa memuat logika bisnis.

use std::env;
use syno_plasticity::SynapseNetwork;
use syno_verbal::VerbalEngine;
use syno_vmm::{PersistentStorage, VirtualMemory};

fn print_banner() {
    println!("============================================================");
    println!("          🧠 SYNAPSIS : NEUROPLASTIC VIRTUAL MACHINE       ");
    println!("   Binary VMM • Synaptic Plasticity • Chaos & Mini-Compiler ");
    println!("============================================================\n");
}

fn print_help() {
    println!("============================================================");
    println!("         📖 SYNO (SYNAPSIS) CLI - PANDUAN PENGGUNAAN        ");
    println!("============================================================");
    println!("Penggunaan:");
    println!("  cargo run -p syno-cli -- [OPSI]");
    println!();
    println!("Opsi Tersedia:");
    println!("  --train <FILE>   Melatih/menelan dataset teks (.txt) atau JSON (.json) ke dalam");
    println!(
        "                   Hierarchical Trie redb sebagai Core Fixed-Weight Associative Memory."
    );
    println!("  --chat           Menjalankan sesi obrolan interaktif otonom berbasis penelusuran");
    println!("                   Hierarchical Trie secara READ-ONLY (aman dari mutasi/peluruhan).");
    println!("  --help, -h       Menampilkan informasi bantuan ini.");
    println!();
    println!("Contoh Perintah:");
    println!("  cargo run -p syno-cli -- --train data/conversations.json");
    println!("  cargo run -p syno-cli -- --chat");
}

fn main() {
    print_banner();

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let db_path = "data/synapsis_persistent.db";

    if let Some(pos) = args.iter().position(|a| a == "--train") {
        if pos + 1 >= args.len() {
            eprintln!("[-] Error: Parameter --train membutuhkan path berkas dataset.");
            eprintln!("    Contoh: cargo run -p syno-cli -- --train data/conversations.json");
            return;
        }
        let target_file = &args[pos + 1];

        println!("[0] INISIALISASI PERSISTENT STORAGE (REDB ENGINE)");
        println!("------------------------------------------------------------");
        let storage = match PersistentStorage::open_or_create(db_path) {
            Ok((st, stats)) => {
                println!("  [+] Database Persisten Berhasil Dimuat : {}", db_path);
                println!(
                    "      - Status Inisialisasi             : {}",
                    if stats.is_new {
                        "DATABASE BARU (Fresh Initialization)"
                    } else {
                        "DATABASE LAMA (Sesi Sebelumnya Berhasil Dipulihkan)"
                    }
                );
                st
            }
            Err(e) => {
                eprintln!("  [-] Gagal inisialisasi persistent storage: {}", e);
                return;
            }
        };

        println!("\n============================================================");
        println!("  🚀 MODE PELATIHAN DATASET MASSAL (HIERARCHICAL TRIE INGESTION)");
        println!("  Target Dataset : {}", target_file);
        println!("  Penyimpanan    : {} (redb ACID)", db_path);
        println!(
            "  Memory Tier    : Core Fixed-Weight Associative Memory ({:.2})",
            SynapseNetwork::CORE_FIXED_WEIGHT
        );
        println!("  RAM Footprint  : O(1) Streaming Lazy Loading");
        println!("============================================================");

        let mut vmm =
            VirtualMemory::with_storage(storage.clone()).unwrap_or_else(|_| VirtualMemory::new());
        let mut network =
            SynapseNetwork::with_storage(storage).unwrap_or_else(|_| SynapseNetwork::new());

        match VerbalEngine::train_dataset_file(target_file, &mut vmm, &mut network) {
            Ok(training_stats) => {
                println!("\n  [+] Ingestion Selesai & Disinkronkan ke redb secara ACID!");
                println!(
                    "      - Berkas Sumber             : {}",
                    training_stats.dataset_source
                );
                println!(
                    "      - Total Baris Dibaca        : {} baris",
                    training_stats.total_lines
                );
                println!(
                    "      - Total Kalimat Diproses    : {} kalimat",
                    training_stats.total_sentences
                );
                println!(
                    "      - Blok Memori Virtual Baru  : {} blok",
                    training_stats.total_memory_blocks
                );
                println!(
                    "      - Cabang Baru Terbentuk     : {} cabang Trie (Tier: Core)",
                    training_stats.new_branches_created
                );
                println!(
                    "      - Cabang Diteguhkan Kembali : {} cabang",
                    training_stats.reinforced_branches
                );
                println!(
                    "      - Total Simpul Akar Trie    : {} kata awal",
                    training_stats.total_trie_roots
                );
                println!(
                    "      - Total Token Terdaftar     : {} kata unik di kamus",
                    training_stats.total_tokens_registered
                );
                println!("      - Status Integritas Bobot   : Fixed-Weight Associative Memory (Kebal Peluruhan)");

                println!("\n[RINGKASAN STRUKTUR POHON PERCABANGAN (HIERARCHICAL TRIE)]");
                println!("------------------------------------------------------------");
                print!("{}", network.render_trie_overview(6, 4));
                println!("============================================================");
                println!("  [✓] Pelatihan selesai! Untuk menguji obrolan otonom, jalankan:");
                println!("      cargo run -p syno-cli -- --chat\n");
            }
            Err(e) => {
                eprintln!("[-] Gagal melakukan pelatihan dataset: {}", e);
            }
        }
        return;
    }

    if args.iter().any(|arg| arg == "--chat") {
        println!("[0] INISIALISASI PERSISTENT STORAGE (REDB ENGINE)");
        println!("------------------------------------------------------------");
        let storage = match PersistentStorage::open_or_create(db_path) {
            Ok((st, stats)) => {
                println!("  [+] Database Persisten Berhasil Dimuat : {}", db_path);
                println!(
                    "      - Status Inisialisasi             : {}",
                    if stats.is_new {
                        "DATABASE BARU (Fresh Initialization)"
                    } else {
                        "DATABASE LAMA (Sesi Sebelumnya Berhasil Dipulihkan)"
                    }
                );
                println!(
                    "      - Blok Memori Virtual di DB       : {} blok (Lazy Loading O(1) RAM)",
                    stats.persisted_memory_blocks
                );
                println!(
                    "      - Sinapsis Terhubung di DB        : {} koneksi tersimpan",
                    stats.persisted_synapses
                );
                st
            }
            Err(e) => {
                eprintln!("  [-] Gagal inisialisasi persistent storage: {}", e);
                return;
            }
        };

        let mut vmm =
            VirtualMemory::with_storage(storage.clone()).unwrap_or_else(|_| VirtualMemory::new());
        let mut network =
            SynapseNetwork::with_storage(storage).unwrap_or_else(|_| SynapseNetwork::new());

        VerbalEngine::run_chat_session(&mut vmm, &mut network);
        return;
    }

    // Default: tampilkan petunjuk dan ringkasan status database
    println!("  [TIP] Mode Eksekusi CLI Tersedia:");
    println!(
        "    * Mode Latih   : cargo run -p syno-cli -- --train <path/file.txt atau file.json>"
    );
    println!("    * Mode Obrolan : cargo run -p syno-cli -- --chat (Inference Read-Only Otonom)");
    println!("    * Bantuan      : cargo run -p syno-cli -- --help\n");

    if let Ok((_storage, stats)) = PersistentStorage::open_or_create(db_path) {
        println!("[STATUS DATABASE PERSISTEN]");
        println!("  - Database Path          : {}", db_path);
        println!(
            "  - Blok Memori Tersimpan  : {}",
            stats.persisted_memory_blocks
        );
        println!("  - Sinapsis Terhubung     : {}", stats.persisted_synapses);
    }
}
