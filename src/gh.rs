//! Integration with the GitHub CLI (`gh`): locate the binary, read the active
//! account offline from `hosts.yml`, switch accounts, and delegate the git
//! credential-helper protocol.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "gh.exe"
    } else {
        "gh"
    }
}

/// Locate the `gh` executable: an explicit configured path, then `PATH`, then
/// well-known install locations. Returns `None` if not found.
pub fn locate_gh(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured {
        if !p.is_empty() {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
    }
    if let Some(p) = which_gh() {
        return Some(p);
    }
    well_known().into_iter().find(|p| p.is_file())
}

fn which_gh() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(exe_name());
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn well_known() -> Vec<PathBuf> {
    if cfg!(windows) {
        let mut v = vec![PathBuf::from(r"C:\Program Files\GitHub CLI\gh.exe")];
        if let Ok(pf) = std::env::var("ProgramFiles") {
            v.push(PathBuf::from(pf).join("GitHub CLI").join("gh.exe"));
        }
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(la).join("GitHub CLI").join("gh.exe"));
        }
        v
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin/gh"),
            PathBuf::from("/usr/local/bin/gh"),
            PathBuf::from("/usr/bin/gh"),
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin/gh"),
        ]
    }
}

/// Resolve the `gh` command string: configured/located path, or `gh` on `PATH`.
pub fn resolve_gh(configured: Option<&str>) -> String {
    locate_gh(configured)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "gh".to_string())
}

/// The gh config directory (honours `GH_CONFIG_DIR`).
pub fn gh_config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("GH_CONFIG_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return PathBuf::from(appdata).join("GitHub CLI");
            }
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("gh");
        }
    }
    if let Some(home) = crate::config::home_dir() {
        return home.join(".config").join("gh");
    }
    PathBuf::from("gh")
}

pub fn hosts_file() -> PathBuf {
    gh_config_dir().join("hosts.yml")
}

/// The currently active account for a host, read offline from `hosts.yml`.
pub fn active_account(host: &str) -> Option<String> {
    let text = std::fs::read_to_string(hosts_file()).ok()?;
    let value: serde_yaml::Value = serde_yaml::from_str(&text).ok()?;
    value
        .get(host)?
        .get("user")?
        .as_str()
        .map(|s| s.to_string())
}

/// All accounts known to `gh` for a host, read offline from `hosts.yml`.
pub fn known_accounts(host: &str) -> Vec<String> {
    let text = match std::fs::read_to_string(hosts_file()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let value: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    value
        .get(host)
        .and_then(|h| h.get("users"))
        .and_then(|u| u.as_mapping())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a configured account name to the exact casing `gh` knows, matching
/// case-insensitively. Falls back to the given name when no match is found so
/// the caller still produces a meaningful error.
pub fn canonical_account(host: &str, account: &str) -> String {
    known_accounts(host)
        .into_iter()
        .find(|a| a.eq_ignore_ascii_case(account))
        .unwrap_or_else(|| account.to_string())
}

/// Switch the active account for a host.
pub fn switch(gh: &str, host: &str, account: &str) -> Result<()> {
    let status = Command::new(gh)
        .args(["auth", "switch", "--hostname", host, "--user", account])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("running `{gh} auth switch`"))?;
    if !status.success() {
        anyhow::bail!(
            "`gh auth switch --hostname {host} --user {account}` exited with {:?} \
             (is that account logged in? run `gh auth status`)",
            status.code()
        );
    }
    Ok(())
}

/// Switch only when the desired account is not already active. The configured
/// account name is resolved to `gh`'s canonical casing first.
pub fn switch_if_needed(gh: &str, host: &str, account: &str) -> Result<()> {
    let target = canonical_account(host, account);
    if active_account(host)
        .map(|a| a.eq_ignore_ascii_case(&target))
        .unwrap_or(false)
    {
        return Ok(());
    }
    switch(gh, host, &target)
}

/// Delegate a credential-helper operation to `gh auth git-credential <op>`,
/// feeding `input` on stdin and letting gh write directly to our stdout.
/// Returns gh's exit code.
pub fn delegate_credential(gh: &str, op: &str, input: &[u8]) -> Result<i32> {
    let mut child = Command::new(gh)
        .args(["auth", "git-credential", op])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning `{gh} auth git-credential {op}`"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).context("writing to gh stdin")?;
    }
    let status = child.wait().context("waiting for gh")?;
    Ok(status.code().unwrap_or(0))
}
