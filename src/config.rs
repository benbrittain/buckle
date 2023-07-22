use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GithubRelease {
    pub owner: String,
    pub repo: String,
    /// Version string regex
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuckleSource {
    Github(GithubRelease),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// Some tools ship as a single file, others as a compressed directory
pub enum PackageType {
    SingleFile,
    ZstdSingleFile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// Archives contain binaries, they are downloaded from sources
pub struct ArchiveConfig {
    pub source: BuckleSource,
    pub package_type: PackageType,
    /// Artifact string regex
    pub artifact_pattern: String,
    // TODO things like checksums,  cache timeouts etc
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// Binaries are runnable from the expanded archive in the cache area
pub struct BinaryConfig {
    pub provided_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// Top level Buckle configuration
pub struct BuckleConfig {
    pub archives: HashMap<String, ArchiveConfig>,
    pub binaries: HashMap<String, BinaryConfig>,
}

impl BuckleConfig {
    // a fallback for when no config is found
    pub fn buck2_latest() -> Self {
        let config_toml = r#"
        [archives.buck2]
        source.github.owner = "facebook"
        source.github.repo = "buck2"
        source.github.version = "latest"
        artifact_pattern = "buck2-%target%.zst"
        package_type = "zstd_single_file"

        [binaries.buck2]
        provided_by = "buck2"
        "#;
        toml::from_str(config_toml).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_buck2_latest() {
        let buckle_config: BuckleConfig = BuckleConfig::buck2_latest();
        assert_eq!(buckle_config.archives.len(), 1);
        assert_eq!(buckle_config.binaries.len(), 1);
    }
}
