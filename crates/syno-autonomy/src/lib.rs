//! Crate: syno-autonomy
//! Lapisan kesadaran dan fase irasional (IrrationalAutonomyEngine), dinamika
//! Arnold circle map dengan rasio emas konjugat, serta dinamika chaos non-linier.

pub mod chaos;
pub mod irrational;

pub use chaos::{fuzzy_threshold_gate, induce_chaotic_drift, sigmoid, ChaosEngine};
pub use irrational::{EndogenousAction, IrrationalAutonomyEngine, ALPHA, BETA, PHI_CONJUGATE};
