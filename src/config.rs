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

    /// Fallback account to switch to when no mapping matches the remote's
    /// host/owner. Host-agnostic; omitted from the file when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_account: Option<String>,

    /// host/owner -> account mappings.
    #[serde(default)]
    pub mappings: Vec<Mapping>,
}

/// A single `host` + `owner` -> `account` mapping.
///
/// `owner` is matched against the remote's first path segment and may be:
/// * a literal org/user name (case-insensitive), or
/// * `*` — a per-host catch-all default, or
/// * a regular expression (e.g. `org1|org2|acme-.*`), matched fully and
///   case-insensitively.
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

    /// Resolve the mapped account for a host/owner.
    ///
    /// Precedence (deterministic, independent of file order):
    /// 1. A literal `owner` that equals the remote owner (case-insensitive).
    /// 2. A regex `owner` that fully matches the remote owner (case-insensitive,
    ///    first match in file order).
    /// 3. A `*` per-host catch-all.
    /// 4. The global `default_account`.
    ///
    /// Returns `None` only when none of the above apply (caller leaves the
    /// active account unchanged). Invalid regex patterns are skipped.
    pub fn lookup(&self, host: &str, owner: &str) -> Option<&str> {
        // 1. Literal exact match wins over any pattern.
        for m in &self.mappings {
            if m.host.eq_ignore_ascii_case(host)
                && is_literal_owner(&m.owner)
                && m.owner.eq_ignore_ascii_case(owner)
            {
                return Some(m.account.as_str());
            }
        }
        // 2. Regex match (first in file order), then 3. `*` catch-all.
        let mut wildcard: Option<&str> = None;
        for m in &self.mappings {
            if !m.host.eq_ignore_ascii_case(host) {
                continue;
            }
            if m.owner == "*" {
                if wildcard.is_none() {
                    wildcard = Some(m.account.as_str());
                }
            } else if !is_literal_owner(&m.owner) && owner_regex_matches(&m.owner, owner) {
                return Some(m.account.as_str());
            }
        }
        // 3. per-host catch-all, else 4. global default.
        wildcard.or(self.default_account.as_deref())
    }
}

/// Whether an `owner` pattern is a plain literal (no regex metacharacters), and
/// so should be compared by case-insensitive equality rather than as a regex.
fn is_literal_owner(owner: &str) -> bool {
    !owner.chars().any(|c| REGEX_META.contains(c))
}

const REGEX_META: &str = r".^$*+?()[]{}|\";

/// Compile `owner` as a fully-anchored, case-insensitive regex and test it
/// against `value`. Invalid patterns never match (they are skipped).
fn owner_regex_matches(owner: &str, value: &str) -> bool {
    let anchored = format!("(?i)^(?:{owner})$");
    match regex::Regex::new(&anchored) {
        Ok(re) => re.is_match(value),
        Err(_) => false,
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
            default_account: None,
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

    fn map(host: &str, owner: &str, account: &str) -> Mapping {
        Mapping {
            host: host.into(),
            owner: owner.into(),
            account: account.into(),
        }
    }

    #[test]
    fn regex_alternation_matches_either_org() {
        let c = Config {
            mappings: vec![map("github.com", "org1|org2", "shared")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "org1"), Some("shared"));
        assert_eq!(c.lookup("github.com", "org2"), Some("shared"));
        assert_eq!(c.lookup("github.com", "org3"), None);
    }

    #[test]
    fn regex_wildcard_matches_prefix() {
        let c = Config {
            mappings: vec![map("github.com", "acme-.*", "acct")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "acme-foo"), Some("acct"));
        assert_eq!(c.lookup("github.com", "acme-"), Some("acct"));
        // Fully anchored: a leading segment that isn't `acme-` must not match.
        assert_eq!(c.lookup("github.com", "notacme-foo"), None);
    }

    #[test]
    fn regex_is_case_insensitive() {
        let c = Config {
            mappings: vec![map("github.com", "Acme-.*", "acct")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "ACME-Bar"), Some("acct"));
    }

    #[test]
    fn literal_exact_wins_over_regex() {
        // Order deliberately puts the regex first to prove precedence.
        let c = Config {
            mappings: vec![
                map("github.com", "acme-.*", "regex_acct"),
                map("github.com", "acme-team", "exact_acct"),
            ],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "acme-team"), Some("exact_acct"));
        assert_eq!(c.lookup("github.com", "acme-other"), Some("regex_acct"));
    }

    #[test]
    fn default_account_used_when_nothing_matches() {
        let c = Config {
            default_account: Some("fallback".into()),
            mappings: vec![map("github.com", "acme-corp", "work")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "acme-corp"), Some("work"));
        assert_eq!(c.lookup("github.com", "unknown-org"), Some("fallback"));
        // Applies to unknown hosts too.
        assert_eq!(c.lookup("ghe.example.com", "whatever"), Some("fallback"));
    }

    #[test]
    fn wildcard_beats_default_account() {
        let c = Config {
            default_account: Some("fallback".into()),
            mappings: vec![map("github.com", "*", "host_default")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "anything"), Some("host_default"));
        assert_eq!(c.lookup("other.com", "anything"), Some("fallback"));
    }

    #[test]
    fn invalid_regex_is_skipped_not_fatal() {
        // `[` is an unterminated character class -> invalid regex.
        let c = Config {
            default_account: Some("fallback".into()),
            mappings: vec![map("github.com", "acme[", "bad")],
            ..Default::default()
        };
        assert_eq!(c.lookup("github.com", "acme["), Some("fallback"));
        assert_eq!(c.lookup("github.com", "acme"), Some("fallback"));
    }

    #[test]
    fn default_account_roundtrips_yaml() {
        let dir = std::env::temp_dir().join(format!("ghas_cfg_def_{}", std::process::id()));
        let path = dir.join("config.yml");
        let c = Config {
            default_account: Some("fallback".into()),
            ..Default::default()
        };
        c.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.default_account.as_deref(), Some("fallback"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
