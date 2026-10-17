//! Module: irrational.rs
//! Modul Otonomi Irasional (Endogenous Irrational Autonomy Engine).
//! Memberikan Syno kemampuan mutasi memori mandiri (Self-Write, Self-Update, Self-Delete/Prune)
//! yang 100% dipicu dari DALAM DIRI Syno sendiri menggunakan dinamika fase bilangan irasional,
//! TANPA perintah eksplisit dari pihak luar/user.

use std::f64::consts::PI;

/// Golden Ratio Conjugate (\frac{\sqrt{5}-1}{2})
pub const PHI_CONJUGATE: f64 = 0.6180339887498949;

/// Konstanta kopling non-linier (Arnold circle map coupling)
pub const ALPHA: f64 = 0.15;

/// Sensitivitas entropi / ketegangan kognitif
pub const BETA: f64 = 0.25;

/// Enum Keputusan Internal Syno (Endogenous Action)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndogenousAction {
    /// Self-Write: Menulis stimulus asing secara mandiri ke Dynamic Tier (>= 0x2000)
    AcquireMemory,
    /// Self-Update: Refleksi internal memperkuat sinapsis Dynamic (+0.08) & promosi ke Core Tier
    ConsolidateAndRewrite,
    /// Self-Delete: Pembersihan mandiri (synaptic pruning) sinapsis Dynamic lemah (< 0.550)
    ApoptosisPrune,
    /// Observe: Keseimbangan normal / observasi pasif
    Homeostasis,
}

impl std::fmt::Display for EndogenousAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndogenousAction::AcquireMemory => write!(f, "AcquireMemory (Self-Write)"),
            EndogenousAction::ConsolidateAndRewrite => {
                write!(f, "ConsolidateAndRewrite (Self-Update)")
            }
            EndogenousAction::ApoptosisPrune => write!(f, "ApoptosisPrune (Self-Delete)"),
            EndogenousAction::Homeostasis => write!(f, "Homeostasis (Observe)"),
        }
    }
}

/// Mesin Otonomi Irasional berbasis Peta Lingkaran Irasional Non-Linier
pub struct IrrationalAutonomyEngine {
    /// Fase kesadaran internal theta_n dalam rentang [0.0, 1.0)
    pub phase_theta: f64,
    /// Ketegangan kognitif internal E_n dalam rentang [0.0, 1.0]
    pub internal_tension: f64,
    /// Penghitung detak kognitif internal
    pub cycle_count: u64,
    /// Buffer internal untuk menyimpan stimulus asing terakhir yang memicu ketegangan
    pub pending_unmapped_input: Option<String>,
    /// Penunjuk alamat alokasi berikutnya di Dynamic Tier (kelipatan 0x80 mulai dari 0x2000)
    pub next_dynamic_addr: usize,
}

impl IrrationalAutonomyEngine {
    /// Inisialisasi engine otonomi irasional dengan kondisi awal seimbang
    pub fn new() -> Self {
        Self {
            phase_theta: 0.0,
            internal_tension: 0.0,
            cycle_count: 0,
            pending_unmapped_input: None,
            next_dynamic_addr: 0x2000,
        }
    }

    /// Inisialisasi engine dengan seed fase dan ketegangan kustom
    pub fn with_seed(theta_seed: f64, initial_tension: f64) -> Self {
        Self {
            phase_theta: theta_seed.rem_euclid(1.0),
            internal_tension: initial_tension.clamp(0.0, 1.0),
            cycle_count: 0,
            pending_unmapped_input: None,
            next_dynamic_addr: 0x2000,
        }
    }

    /// Menyelaraskan pointer next_dynamic_addr agar tidak menimpa slot yang sudah ada
    pub fn sync_next_dynamic_addr(&mut self, highest_addr: usize) {
        if highest_addr >= self.next_dynamic_addr {
            self.next_dynamic_addr = ((highest_addr / 0x80) + 1) * 0x80;
        }
    }

    /// Mendaftarkan stimulus asing (OOV / Unmapped) ke dalam buffer pending
    pub fn register_unmapped_stimulus(&mut self, stimulus: &str) {
        self.pending_unmapped_input = Some(stimulus.trim().to_string());
    }

    /// Merelaksasi ketegangan internal setelah memori berhasil diserap ke Dynamic Tier
    pub fn on_memory_acquired(&mut self) {
        self.internal_tension = (self.internal_tension - 0.35).max(0.0);
        self.pending_unmapped_input = None;
    }

    /// Persamaan Evolusi Fase:
    /// Setiap kali Syno memproses satu siklus input, perbarui E_n, lalu hitung fase berikutnya:
    /// theta_{n+1} = (theta_n + phi + alpha * sin(2*pi*theta_n) + beta * E_n) mod 1.0
    pub fn step_pulse(&mut self, is_oov: bool) -> EndogenousAction {
        // 1. Perbarui ketegangan kognitif E_n
        if is_oov {
            self.internal_tension = (self.internal_tension + 0.40).clamp(0.0, 1.0);
        } else if self.pending_unmapped_input.is_some() {
            // Jika masih ada stimulus unmapped yang belum terserap, pertahankan ketegangan
            self.internal_tension = (self.internal_tension - 0.02).clamp(0.35, 1.0);
        } else {
            // Berhasil menjawab dari memori -> ketegangan mereda perlahan
            self.internal_tension = (self.internal_tension - 0.05).clamp(0.0, 1.0);
        }

        // 2. Hitung fase berikutnya menggunakan Peta Lingkaran Irasional Non-Linier
        let non_linear_drift = ALPHA * (2.0 * PI * self.phase_theta).sin();
        let entropy_drive = BETA * self.internal_tension;
        let delta = PHI_CONJUGATE + non_linear_drift + entropy_drive;

        self.phase_theta = (self.phase_theta + delta).rem_euclid(1.0);
        self.cycle_count += 1;

        // 3. Evaluasi Keputusan Endogen Murni berdasarkan theta_{n+1} dan E_n
        if (0.618..=0.780).contains(&self.phase_theta) && self.internal_tension >= 0.35 {
            EndogenousAction::AcquireMemory
        } else if (0.820..=0.920).contains(&self.phase_theta) {
            EndogenousAction::ConsolidateAndRewrite
        } else if self.phase_theta > 0.920 {
            EndogenousAction::ApoptosisPrune
        } else {
            EndogenousAction::Homeostasis
        }
    }
}

impl Default for IrrationalAutonomyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irrational_winding_ergodicity_and_triggers() {
        let mut engine = IrrationalAutonomyEngine::new();

        let mut acquire_count = 0;
        let mut consolidate_count = 0;
        let mut prune_count = 0;
        let mut homeostasis_count = 0;

        let mut visited_phases = std::collections::HashSet::new();

        // Jalankan 150 siklus pulsa irasional dengan variasi input OOV dan non-OOV
        for i in 0..150 {
            let is_oov = (i % 7 == 0) || (i % 11 == 0);
            let action = engine.step_pulse(is_oov);

            let quantized_phase = (engine.phase_theta * 10_000.0).round() as i64;
            visited_phases.insert(quantized_phase);

            match action {
                EndogenousAction::AcquireMemory => {
                    acquire_count += 1;
                    engine.on_memory_acquired();
                }
                EndogenousAction::ConsolidateAndRewrite => {
                    consolidate_count += 1;
                }
                EndogenousAction::ApoptosisPrune => {
                    prune_count += 1;
                }
                EndogenousAction::Homeostasis => {
                    homeostasis_count += 1;
                }
            }
        }

        assert!(
            visited_phases.len() > 100,
            "Fase theta harus memiliki variasi ergodik tinggi tanpa terjebak siklus berulang pendek! Ditemukan: {}",
            visited_phases.len()
        );

        assert!(
            acquire_count > 0,
            "AcquireMemory (Self-Write) harus terpicu secara otonom! Ditemukan: {}",
            acquire_count
        );
        assert!(
            consolidate_count > 0,
            "ConsolidateAndRewrite (Self-Update) harus terpicu secara otonom! Ditemukan: {}",
            consolidate_count
        );
        assert!(
            prune_count > 0,
            "ApoptosisPrune (Self-Delete) harus terpicu secara otonom! Ditemukan: {}",
            prune_count
        );
        assert!(
            homeostasis_count > 0,
            "Homeostasis (Observe) harus teramati saat sistem seimbang! Ditemukan: {}",
            homeostasis_count
        );
    }
}
