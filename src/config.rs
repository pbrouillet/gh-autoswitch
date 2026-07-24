//! Configuration model: YAML load/save, path resolution and mapping lookup.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The on-disk configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the `gh` executable (the "credential manager"). Inferred on
    /// install; omitted from the file when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gh_path: Option<String>,

    /// host/owner -> account mappings.
    #[serde(default)]
    pub mappings: Vec<Mapping>,
}

/// A single `host` + `owner` -> `account` mapping. `owner` may be `*` to act as
/// a per-host default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub host: String,
    pub owner: String,
    pub account: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if text.trim().is_empty() {
            return Ok(Config::default());
        }
        let cfg: Config =
            serde_yaml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = serde_yaml::to_string(self).context("serializing config")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Resolve the mapped account for a host/owner: an exact `host`+`owner`
    /// match wins over a `host`+`*` wildcard. Returns `None` when nothing
    /// matches (caller should leave the active account unchanged).
    pub fn lookup(&self, host: &str, owner: &str) -> Option<&str> {
        let mut wildcard: Option<&str> = None;
        for m in &self.mappings {
            if !m.host.eq_ignore_ascii_case(host) {
                continue;
            }
            if m.owner.eq_ignore_ascii_case(owner) {
                return Some(m.account.as_str());
            }
            if m.owner == "*" {
                wildcard = Some(m.account.as_str());
            }
        }
        wildcard
    }
}

/// Resolve the config file path (honours `GH_AUTOSWITCH_CONFIG`).
pub fn config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("GH_AUTOSWITCH_CONFIG") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    Ok(config_dir()?.join("gh-autoswitch").join("config.yml"))
}

fn config_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return Ok(PathBuf::from(appdata));
            }
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Ok(PathBuf::from(xdg));
        }
    }
    let home = home_dir().context("cannot determine home directory")?;
    Ok(home.join(".config"))
}

/// The user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        Config {
            gh_path: None,
            mappings: vec![
                Mapping {
                    host: "github.com".into(),
                    owner: "acme-corp".into(),
                    account: "alice_work".into(),
                },
                Mapping {
                    host: "github.com".into(),
                    owner: "*".into(),
                    account: "alice_personal".into(),
                },
            ],
        }
    }

    #[test]
    fn exact_match_wins_over_wildcard() {
        let c = sample();
        assert_eq!(c.lookup("github.com", "acme-corp"), Some("alice_work"));
    }

    #[test]
    fn wildcard_used_for_unknown_owner() {
        let c = sample();
        assert_eq!(c.lookup("github.com", "someone"), Some("alice_personal"));
    }

    #[test]
    fn no_match_for_other_host() {
        let c = sample();
        assert_eq!(c.lookup("ghe.example.com", "acme-corp"), None);
    }

    #[test]
    fn lookup_is_case_insensitive_for_host_and_owner() {
        let c = sample();
        assert_eq!(c.lookup("GitHub.com", "ACME-Corp"), Some("alice_work"));
    }

    #[test]
    fn roundtrip_yaml() {
        let dir = std::env::temp_dir().join(format!("ghas_cfg_{}", std::process::id()));
        let path = dir.join("config.yml");
        let mut c = sample();
        c.gh_path = Some("/usr/bin/gh".into());
        c.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.gh_path.as_deref(), Some("/usr/bin/gh"));
        assert_eq!(loaded.mappings.len(), 2);
        assert_eq!(loaded.lookup("github.com", "acme-corp"), Some("alice_work"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_default() {
        let c = Config::load_from(Path::new("/no/such/file.yml")).unwrap();
        assert!(c.mappings.is_empty());
        assert!(c.gh_path.is_none());
    }
}
