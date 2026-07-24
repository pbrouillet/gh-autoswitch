//! Install/uninstall the git credential helper and print diagnostics.

use crate::config::{self, Config};
use crate::gh;
use anyhow::{Context, Result};
use std::process::Command;

fn scope_flag(global: bool) -> &'static str {
    if global {
        "--global"
    } else {
        "--local"
    }
}

/// Configure git to use this binary as the credential helper for `host`, enable
/// `useHttpPath`, and infer/persist the path to `gh` (the credential manager).
pub fn install(host: &str, global: bool) -> Result<()> {
    let scope = scope_flag(global);
    let exe = std::env::current_exe().context("cannot determine own executable path")?;
    let exe_fwd = exe.to_string_lossy().replace('\\', "/");
    let helper = format!("!\"{exe_fwd}\" git-credential");
    let helper_key = format!("credential.https://{host}.helper");
    let uhp_key = format!("credential.https://{host}.useHttpPath");

    // Clear any existing/inherited helpers for this host, then set ours.
    let _ = git(&["config", scope, "--unset-all", &helper_key]);
    git(&["config", scope, "--add", &helper_key, ""])?;
    git(&["config", scope, "--add", &helper_key, &helper])?;
    git(&["config", scope, &uhp_key, "true"])?;

    // Infer + persist the credential-manager (gh) path.
    let mut cfg = Config::load().unwrap_or_default();
    let mut gh_note = cfg.gh_path.clone();
    if cfg.gh_path.is_none() {
        if let Some(p) = gh::locate_gh(None) {
            let s = p.to_string_lossy().into_owned();
            cfg.gh_path = Some(s.clone());
            gh_note = Some(s);
            cfg.save().ok();
        }
    }

    println!("gh-autoswitch: installed credential helper for https://{host} ({scope})");
    println!("  helper : {helper}");
    match &gh_note {
        Some(p) => println!("  gh path: {p}"),
        None => println!("  gh path: NOT FOUND (set it via `gh-autoswitch tui`)"),
    }
    println!("  config : {}", config::config_path()?.display());
    Ok(())
}

/// Remove the git credential-helper configuration for `host`.
pub fn uninstall(host: &str, global: bool) -> Result<()> {
    let scope = scope_flag(global);
    let helper_key = format!("credential.https://{host}.helper");
    let uhp_key = format!("credential.https://{host}.useHttpPath");
    let _ = git(&["config", scope, "--unset-all", &helper_key]);
    let _ = git(&["config", scope, "--unset", &uhp_key]);
    println!("gh-autoswitch: removed credential helper for https://{host} ({scope})");
    Ok(())
}

/// Print diagnostics: config, mappings, resolved gh path, active account and the
/// effective git credential configuration.
pub fn doctor(host: &str) -> Result<()> {
    let cfg_path = config::config_path()?;
    let cfg = Config::load().unwrap_or_default();

    println!("gh-autoswitch doctor");
    println!("  config file : {}", cfg_path.display());
    if cfg.mappings.is_empty() {
        println!("  mappings    : (none)");
    } else {
        println!("  mappings    :");
        for m in &cfg.mappings {
            println!("      {}/{} = {}", m.host, m.owner, m.account);
        }
    }
    let gh_bin = gh::locate_gh(cfg.gh_path.as_deref());
    println!(
        "  gh path     : {}",
        gh_bin
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "NOT FOUND".to_string())
    );
    println!("  hosts.yml   : {}", gh::hosts_file().display());
    println!(
        "  active[{host}]: {}",
        gh::active_account(host).unwrap_or_else(|| "(unknown)".to_string())
    );
    println!(
        "  git helper  : {}",
        git_get(&format!("credential.https://{host}.helper"))
            .unwrap_or_else(|| "(unset)".to_string())
    );
    println!(
        "  useHttpPath : {}",
        git_get(&format!("credential.https://{host}.useHttpPath"))
            .unwrap_or_else(|| "(unset)".to_string())
    );
    Ok(())
}

fn git(args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .status()
        .context("running `git`")?;
    if !status.success() {
        anyhow::bail!("`git {}` exited with {:?}", args.join(" "), status.code());
    }
    Ok(())
}

fn git_get(key: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["config", "--get-all", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s.replace('\n', " | "))
    }
}
