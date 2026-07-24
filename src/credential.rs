//! The git credential-helper entry point (`git-credential get|store|erase`).

use crate::config::Config;
use crate::gh;
use anyhow::Result;
use std::io::Read;

/// Run the credential helper for the given operation. Always delegates to `gh`
/// so git receives a valid credential, even if switching fails (fail-safe).
pub fn run(op: &str) -> Result<i32> {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input)?;

    let cfg = Config::load().unwrap_or_default();
    let gh_bin = gh::resolve_gh(cfg.gh_path.as_deref());

    if op == "get" {
        // Best-effort: never let switching block the git operation, but do
        // report failures on stderr so a mis-switch is diagnosable (git
        // surfaces credential-helper stderr to the user).
        if let Err(e) = maybe_switch(&cfg, &gh_bin, &input) {
            eprintln!("gh-autoswitch: could not switch account: {e:#}");
        }
    }

    gh::delegate_credential(&gh_bin, op, &input)
}

fn maybe_switch(cfg: &Config, gh_bin: &str, input: &[u8]) -> Result<()> {
    let (host, owner) = match parse_owner(input) {
        Some(v) => v,
        None => return Ok(()),
    };
    if let Some(account) = cfg.lookup(&host, &owner) {
        gh::switch_if_needed(gh_bin, &host, account)?;
    }
    Ok(())
}

/// Parse the `host=` and `path=` lines of a git credential request and derive
/// `(host, owner)` where owner is the first path segment.
fn parse_owner(input: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(input);
    let mut host: Option<String> = None;
    let mut path: Option<String> = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("host=") {
            host = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("path=") {
            path = Some(v.trim().to_string());
        }
    }
    let host = host?;
    let path = path?;
    let owner = path.split('/').next().unwrap_or("").to_string();
    if host.is_empty() || owner.is_empty() {
        return None;
    }
    Some((host, owner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_host_and_owner() {
        let input = b"protocol=https\nhost=github.com\npath=acme-corp/repo.git\n\n";
        assert_eq!(
            parse_owner(input),
            Some(("github.com".to_string(), "acme-corp".to_string()))
        );
    }

    #[test]
    fn none_without_path() {
        let input = b"protocol=https\nhost=github.com\n\n";
        assert_eq!(parse_owner(input), None);
    }

    #[test]
    fn owner_is_first_segment() {
        let input = b"host=github.com\npath=org/sub/repo.git\n";
        assert_eq!(
            parse_owner(input).map(|(_, o)| o),
            Some("org".to_string())
        );
    }
}
