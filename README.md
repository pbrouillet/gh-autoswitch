# gh-autoswitch

Automatically run `gh auth switch` to select the right GitHub account **before
every HTTPS git remote operation** (`push`, `pull`, `fetch`, `clone`, …) — based
on the remote's owner/org.

If you juggle multiple GitHub accounts (personal + work, or several enterprise
hosts), the *active* `gh` account decides which token `gh` hands to git as a
credential. Pick the wrong one and your push fails or lands as the wrong
identity. `gh-autoswitch` removes the manual `gh auth switch` step.

## How it works

Git has **no** `pre-fetch`/`pre-pull` hook (only `pre-push`), so a plain git hook
can't cover every remote operation. Instead `gh-autoswitch` installs itself as a
**git credential helper**, which git invokes on *every* HTTPS remote operation.
On each call it:

1. Reads the credential request git sends on stdin (`protocol`, `host`, `path`).
2. Derives the **owner/org** from the path and looks it up in your config file.
3. Runs `gh auth switch --hostname <host> --user <account>` — only if that
   account isn't already active (checked offline via gh's `hosts.yml`).
4. Delegates to `gh auth git-credential`, so git still receives a valid token.

Owner-based mapping needs git to pass the repo path to the helper, so the
installer also sets `credential.<host>.useHttpPath=true` (scoped to the host).

```
git fetch ──▶ git calls credential helper ──▶ gh-autoswitch
                                                 ├─ owner = "acme-corp"
                                                 ├─ config: github.com/acme-corp = alice_work
                                                 ├─ gh auth switch --user alice_work   (if needed)
                                                 └─ gh auth git-credential get ──▶ token ──▶ git
```

## Requirements

- [`gh`](https://cli.github.com/) (GitHub CLI) with your accounts already logged
  in (`gh auth login` for each).
- `git`.
- macOS/Linux: `bash` + `awk`. Windows: PowerShell (bundled) — the git
  credential helper runs via `powershell`.

## Install

Clone the repo, then run the installer for your platform.

**macOS / Linux (bash):**

```bash
./install.sh                 # global, github.com
./install.sh --host ghe.corp.example.com
./install.sh --local         # only the current repo
```

**Windows (PowerShell):**

```powershell
.\install.ps1                 # global, github.com
.\install.ps1 --host ghe.corp.example.com
.\install.ps1 --local
```

This writes, for example:

```ini
[credential "https://github.com"]
    helper =
    helper = !"/path/to/bin/gh-autoswitch" git-credential
    useHttpPath = true
```

(The empty `helper =` clears any inherited helpers so gh-autoswitch is
authoritative for that host.)

Optionally add `bin/` to your `PATH` so you can run `gh-autoswitch` directly.

## Configure

Create the config file mapping `host/owner` → gh account username:

- Linux/macOS: `${XDG_CONFIG_HOME:-~/.config}/gh-autoswitch/config`
- Windows: `%APPDATA%\gh-autoswitch\config`
- or set `GH_AUTOSWITCH_CONFIG` to any path.

```ini
# Exact owner/org wins over the host wildcard
github.com/acme-corp   = alice_work
github.com/alice       = alice_personal
github.com/*           = alice_personal      # per-host default

ghe.corp.example.com/* = alice_corp
```

See [`config.example`](./config.example). If nothing matches, the active account
is left unchanged (no-op).

## Verify

```bash
gh-autoswitch doctor            # or: bin/gh-autoswitch doctor
```

Shows the resolved config, the active account per host, and the effective git
credential configuration. Then just `git fetch` / `git push` as usual.

## Uninstall

```bash
./bin/gh-autoswitch uninstall             # bash
.\bin\gh-autoswitch.ps1 uninstall         # PowerShell
```

Add `--host` / `--local` / `--global` to match how you installed.

## Commands

| Command | Description |
| --- | --- |
| `git-credential <get\|store\|erase>` | Credential-helper protocol (called by git). |
| `install [--host H] [--local\|--global]` | Configure git to use the helper. |
| `uninstall [--host H] [--local\|--global]` | Remove the git configuration. |
| `doctor [host]` | Print diagnostics. |

## Notes & limitations

- **SSH is out of scope.** `gh auth switch` only affects the HTTPS token `gh`
  vends as a credential helper; SSH remotes don't invoke credential helpers.
  Use HTTPS remotes for autoswitching.
- **Fail-safe.** Any error (missing config/mapping/`gh`) falls back to plain
  `gh auth git-credential`, so a misconfiguration never breaks git.
- **No secrets stored.** The config holds only account *usernames*; tokens stay
  in gh's own secure storage.
- **Performance.** The helper runs on every remote op and reads gh's `hosts.yml`
  offline; it only shells out to `gh auth switch` when the account must change.

## Tests

No real GitHub auth is touched — the suites use a mock `gh` on `PATH`.

```bash
bash test/run.sh                                   # bash helper (8 checks)
```
```powershell
Invoke-Pester -Path test\gh-autoswitch.Tests.ps1   # PowerShell helper (Pester 5)
```

## License

[MIT](./LICENSE)
