# gh-autoswitch

A single cross-platform **Rust** binary that automatically runs `gh auth switch`
to select the right GitHub account **before every HTTPS git remote operation**
(`push`, `pull`, `fetch`, `clone`, …), plus a **Ratatui TUI** to scaffold/edit
the config and detect the path to `gh`.

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
2. Derives the **owner/org** from the path and looks it up in the YAML config.
3. Runs `gh auth switch --hostname <host> --user <account>` — only if that
   account isn't already active (checked offline via gh's `hosts.yml`).
4. Delegates to `gh auth git-credential`, so git still receives a valid token.

Owner-based mapping needs git to pass the repo path to the helper, so the
installer also sets `credential.<host>.useHttpPath=true` (scoped to the host).

```
git fetch ─▶ git calls credential helper ─▶ gh-autoswitch
                                              ├─ owner = "acme-corp"
                                              ├─ config: github.com/acme-corp → alice_work
                                              ├─ gh auth switch --user alice_work   (if needed)
                                              └─ gh auth git-credential get ─▶ token ─▶ git
```

## Requirements

- [`gh`](https://cli.github.com/) with your accounts logged in (`gh auth login`).
- `git`.
- To build: a Rust toolchain (`cargo`).

## Build & install

```bash
cargo build --release
# binary: target/release/gh-autoswitch(.exe)
```

Wire it up as git's credential helper (also infers & stores the path to `gh`):

```bash
target/release/gh-autoswitch install            # global, github.com
target/release/gh-autoswitch install --host ghe.corp.example.com
target/release/gh-autoswitch install --local    # only the current repo
```

This writes, for example:

```ini
[credential "https://github.com"]
    helper =
    helper = !"/path/to/gh-autoswitch" git-credential
    useHttpPath = true
```

(The empty `helper =` clears any inherited helpers.) Put the binary on your
`PATH` so you can just run `gh-autoswitch`.

## Configure (TUI)

Just run the binary with no arguments to open the editor:

```bash
gh-autoswitch          # or: gh-autoswitch tui
```

- Table of `host / owner → account` mappings.
- Keys: `↑/↓` select, `a` add, `e` edit, `d` delete, `D` **set default account**,
  `g` **detect gh path**, `s` save, `q` quit (prompts if unsaved).
- The header shows the detected/​set `gh_path`, the `default_account`, and the
  config file location.

### Config file (YAML)

Location: `%APPDATA%\gh-autoswitch\config.yml` (Windows) /
`${XDG_CONFIG_HOME:-~/.config}/gh-autoswitch/config.yml`; override with
`GH_AUTOSWITCH_CONFIG`. See [`config.example.yml`](./config.example.yml).

```yaml
gh_path: C:\Program Files\GitHub CLI\gh.exe   # inferred path to gh
default_account: alice_personal               # fallback when nothing matches
mappings:
  - host: github.com
    owner: acme-corp             # exact owner (case-insensitive)
    account: alice_work
  - host: github.com
    owner: "acme-.*|widgets-inc" # regex: several orgs share one account
    account: alice_work
  - host: github.com
    owner: "*"                   # per-host default
    account: alice_personal
```

**`owner` matching.** Each mapping's `owner` may be an exact org/user name, a
**regular expression** (matched fully and case-insensitively, e.g.
`org1|org2` or `acme-.*`), or `*` for a per-host catch-all. Resolution
precedence:

1. exact name → 2. regex → 3. per-host `*` → 4. `default_account`.

If none of these apply (no mapping and no `default_account`), the active account
is left unchanged (no-op). Invalid regex patterns are skipped.

## Verify

```bash
gh-autoswitch doctor
```

Shows the resolved config, mappings, the detected `gh` path, the active account
per host, and the effective git credential configuration. Then just `git fetch`
/ `git push` as usual.

## Uninstall

```bash
gh-autoswitch uninstall            # add --host / --local / --global to match
```

## Commands

| Command | Description |
| --- | --- |
| `tui` (default, no args) | Ratatui config editor. |
| `git-credential <get\|store\|erase>` | Credential-helper protocol (called by git). |
| `install [--host H] [--local\|--global]` | Configure git + infer `gh` path. |
| `uninstall [--host H] [--local\|--global]` | Remove the git configuration. |
| `doctor [host]` | Print diagnostics. |

## Tests

```bash
cargo test        # unit tests: config lookup/roundtrip, credential parsing
```

## Notes & limitations

- **SSH is out of scope.** `gh auth switch` only affects the HTTPS token `gh`
  vends as a credential helper; SSH remotes don't invoke credential helpers.
- **Fail-safe.** Any error (missing config/mapping/`gh`) falls back to plain
  `gh auth git-credential`, so a misconfiguration never breaks git.
- **No secrets stored.** The config holds only account usernames and the `gh`
  path; tokens stay in gh's own secure storage.
- **Performance.** The helper reads gh's `hosts.yml` offline and only shells out
  to `gh auth switch` when the account must actually change.

## License

[MIT](./LICENSE)
