# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

Kami sangat mengutamakan keamanan dan integritas proyek `synapsis`. Jika Anda menemukan potensi kerentanan keamanan (*security vulnerability*), mohon untuk **tidak** membuka isu publik di GitHub Issues.

Silakan laporkan secara bertanggung jawab melalui:
- **GitHub Security Advisories**: Buat laporan privat melalui tab [Security Advisories](https://github.com/ratuaurion-dev/synapsis/security/advisories/new) di repositori ini.
- Atau kirimkan email ke: `security@ratuaurion.dev` atau melalui akun GitHub `@ratuaurion-dev`.

### Informasi yang Dibutuhkan:
1. Deskripsi kerentanan.
2. Langkah-langkah untuk mereproduksi (Proof of Concept).
3. Potensi dampak terhadap integritas memori virtual, database redb, atau eksekusi runtime.

Kami akan meninjau dan merespons dalam waktu 48 jam serta merilis patch perbaikan secepat mungkin.

## Prinsip Keamanan Sistem
- **Zero Unsafe Code**: Proyek ini dibangun dengan `100% safe Rust` di seluruh level logika aplikasi dan komputasi memori.
- **ACID Transaction Isolation**: Seluruh penyimpanan persisten diisolasi dalam transaksi ACID embedded database `redb`.
- **Bounds Checking**: Virtual Memory Manager (`syno-vmm`) secara ketat memvalidasi batas alamat memori (`0x0000..TOTAL_MEMORY`) untuk mencegah *out-of-bounds read/write* dan *buffer overflow*.
- **Read-Only Chat Inference**: Mode inferensi obrolan terisolasi secara murni (Read-Only) sehingga input pengguna tidak dapat merusak atau memanipulasi struktur ingatan permanen.
