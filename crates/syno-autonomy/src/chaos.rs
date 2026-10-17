//! Module: chaos.rs
//! Modul dinamika chaos berbasis Chaotic Logistic Map, aktivasi sigmoidal, dan chaotic drift.

use syno_plasticity::SynapseNetwork;
use syno_vmm::VirtualMemory;

/// Pembangkit dinamika non-linier berbasis Chaotic Logistic Map.
/// Formula: x_{n+1} = r * x_n * (1 - x_n)
pub struct ChaosEngine {
    pub r: f64,
    pub state: f64,
}

impl ChaosEngine {
    pub fn new(seed: f64, r: f64) -> Self {
        let initial_state = if seed <= 0.0 || seed >= 1.0 {
            0.54321
        } else {
            seed
        };
        Self {
            r: r.clamp(3.57, 4.0),
            state: initial_state,
        }
    }

    /// Menghitung iterasi chaos berikutnya [0.0 .. 1.0]
    pub fn step(&mut self) -> f64 {
        self.state = self.r * self.state * (1.0 - self.state);
        self.state
    }

    /// Menghasilkan byte pseudorandom chaos
    pub fn next_byte(&mut self) -> u8 {
        (self.step() * 255.0).floor() as u8
    }
}

impl Default for ChaosEngine {
    fn default() -> Self {
        Self::new(0.618033, 3.9876)
    }
}

/// Fungsi aktivasi non-linier Sigmoid untuk transisi nilai lembut
pub fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Gerbang logika non-linier: Mengambil keputusan fuzzy dari nilai byte memori
pub fn fuzzy_threshold_gate(val: u8, threshold: f64) -> (bool, f64) {
    let normalized = (val as f64 - 128.0) / 32.0;
    let activation = sigmoid(normalized);
    (activation >= threshold, activation)
}

/// Menginduksi pergeseran chaos (chaotic drift) pada memori dan bobot sinapsis
pub fn induce_chaotic_drift(
    engine: &mut ChaosEngine,
    memory: &mut VirtualMemory,
    network: &mut SynapseNetwork,
    target_addr: usize,
    sample_size: usize,
) {
    for i in 0..sample_size {
        let addr = target_addr + i;
        if let Ok(current_byte) = memory.read_byte(addr) {
            let chaos_val = engine.next_byte();
            let drifted = current_byte ^ (chaos_val & 0x0F);
            let _ = memory.write_byte(addr, drifted);
        }
    }

    for synapse in network.synapses.values_mut() {
        if synapse.is_active {
            let perturbation = (engine.step() - 0.5) * 0.1;
            synapse.weight = (synapse.weight + perturbation as f32).clamp(0.05, 1.0);
        }
    }
}
