mod config;
mod credential;
mod gh;
mod install;
mod tui;

use anyhow::Result;

fn main() {
    let code = match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("gh-autoswitch: error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match cmd {
        "git-credential" => {
            let op = args.get(1).map(|s| s.as_str()).unwrap_or("");
            credential::run(op)
        }
        "install" => {
            let (host, global) = parse_scope(&args[1..]);
            install::install(&host, global)?;
            Ok(0)
        }
        "uninstall" => {
            let (host, global) = parse_scope(&args[1..]);
            install::uninstall(&host, global)?;
            Ok(0)
        }
        "doctor" => {
            let host = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| "github.com".to_string());
            install::doctor(&host)?;
            Ok(0)
        }
        "tui" | "edit" => {
            tui::run()?;
            Ok(0)
        }
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(0)
        }
        other => {
            eprintln!("gh-autoswitch: unknown command: {other}");
            print_usage();
            Ok(2)
        }
    }
}

/// Parse `--host H|--host=H` and `--local|--global` (default: github.com, global).
fn parse_scope(args: &[String]) -> (String, bool) {
    let mut host = "github.com".to_string();
    let mut global = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--local" => global = false,
            "--global" => global = true,
            "--host" if i + 1 < args.len() => {
                host = args[i + 1].clone();
                i += 1;
            }
            s if s.starts_with("--host=") => host = s["--host=".len()..].to_string(),
            _ => {}
        }
        i += 1;
    }
    (host, global)
}

fn print_usage() {
    println!(
        "gh-autoswitch — auto-switch gh account for git remote operations\n\
\n\
USAGE\n\
  gh-autoswitch tui                                Launch the config editor\n\
  gh-autoswitch git-credential <get|store|erase>   Credential helper (called by git)\n\
  gh-autoswitch install   [--host H] [--local|--global]\n\
  gh-autoswitch uninstall [--host H] [--local|--global]\n\
  gh-autoswitch doctor    [host]\n\
  gh-autoswitch help\n"
    );
}
