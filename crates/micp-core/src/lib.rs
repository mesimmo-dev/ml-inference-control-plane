//! Shared domain types and closed-form serving models for the ML
//! inference control plane.
//!
//! Inventory types ([`ModelProfile`]) remain the compact config/API
//! snapshot. The production engine uses [`InferenceCandidate`],
//! [`WorkloadProfile`], and [`estimate_route`].

mod domain;
mod estimate;
mod inventory;
pub mod scenarios;
mod units;

pub use domain::*;
pub use estimate::*;
pub use inventory::*;
pub use scenarios::*;
pub use units::*;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MicpError {
    #[error("no feasible model for the given constraints")]
    NoFeasibleModel,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, MicpError>;
