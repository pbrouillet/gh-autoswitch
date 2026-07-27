# winget packaging

Manifests and automation to publish `gh-autoswitch` to the
[Windows Package Manager](https://learn.microsoft.com/windows/package-manager/)
community repo, so users can run:

```powershell
winget install pbrouillet.gh-autoswitch
```

The Windows release assets (`*-pc-windows-msvc.zip`) are used directly as
**portable (nested-zip) installers** — `gh-autoswitch.exe` sits at the zip root
and is exposed as the `gh-autoswitch` command alias.

## One-time setup

1. **Fork** [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs)
   into the `pbrouillet` account (the automation opens PRs from this fork).
2. Create a **classic Personal Access Token** with the **`public_repo`** scope
   (fine-grained tokens are *not* supported by the releaser action).
3. Add it as a repository **secret named `WINGET_TOKEN`**
   (Settings → Secrets and variables → Actions).

## First submission (manual, once)

winget-pkgs generally needs the first version of a package to be submitted
manually before automation can post updates. Easiest is `wingetcreate` on
Windows:

```powershell
winget install Microsoft.WingetCreate
wingetcreate new `
  https://github.com/pbrouillet/gh-autoswitch/releases/download/v1.1.1/gh-autoswitch-v1.1.1-x86_64-pc-windows-msvc.zip `
  https://github.com/pbrouillet/gh-autoswitch/releases/download/v1.1.1/gh-autoswitch-v1.1.1-aarch64-pc-windows-msvc.zip
```

When prompted, use:

- **PackageIdentifier:** `pbrouillet.gh-autoswitch`
- **InstallerType:** `zip`, **NestedInstallerType:** `portable`
- **RelativeFilePath:** `gh-autoswitch.exe`, **PortableCommandAlias:** `gh-autoswitch`
- Architectures **x64** and **arm64**

Alternatively, the ready-made manifest set lives in this folder at
[`manifests/p/pbrouillet/gh-autoswitch/1.1.1/`](./manifests/p/pbrouillet/gh-autoswitch/1.1.1/).
Copy that `1.1.1` folder into your winget-pkgs fork under the same
`manifests/p/pbrouillet/gh-autoswitch/` path, validate, and open a PR:

```powershell
winget validate --manifest .\packaging\winget\manifests\p\pbrouillet\gh-autoswitch\1.1.1\
winget install --manifest .\packaging\winget\manifests\p\pbrouillet\gh-autoswitch\1.1.1\   # local test
```

> Keep the `InstallerSha256` values in sync with the release. They match the
> `.sha256` sidecar assets published alongside each `.zip`.

## Subsequent releases (automatic)

The `winget` job in [`.github/workflows/release.yml`](../../.github/workflows/release.yml)
runs `vedantmgoyal9/winget-releaser` after each tagged release and opens an
update PR to winget-pkgs automatically. It is a no-op until `WINGET_TOKEN` is
configured.
