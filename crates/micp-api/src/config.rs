use std::path::{Path, PathBuf};

use micp_core::{MicpError, ModelProfile, ObjectiveWeights, Result};
use micp_policy::Policy;
use serde::Deserialize;

pub const DEFAULT_CONFIG: &str = include_str!("../../../configs/default.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SloFile {
    pub name: String,
    pub success_target: f64,
    pub latency_ms: f64,
    pub min_error_budget: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FileConfig {
    pub server: ServerConfig,
    pub weights: ObjectiveWeights,
    pub slo: SloFile,
    pub models: Vec<ModelProfile>,
    #[serde(default)]
    pub policies: Vec<Policy>,
}

impl FileConfig {
    pub fn parse(raw: &str) -> Result<Self> {
        toml::from_str(raw).map_err(|e| MicpError::InvalidConfig(e.to_string()))
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| MicpError::InvalidConfig(format!("read {}: {e}", path.display())))?;
        Self::parse(&raw)
    }

    pub fn load() -> Result<(Self, PathBuf)> {
        let path = std::env::var("MICP_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("configs/default.toml"));
        if path.exists() {
            Ok((Self::from_path(&path)?, path))
        } else {
            Ok((Self::parse(DEFAULT_CONFIG)?, path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_config_parses() {
        let cfg = FileConfig::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.models.len(), 3);
        assert_eq!(cfg.policies.len(), 2);
        assert!(cfg.weights.latency > 0.0);
        assert_eq!(cfg.slo.success_target, 0.999);
    }
}
