<#
.SYNOPSIS
  gh-autoswitch — auto-switch the active `gh` account before HTTPS git remote
  operations (push/pull/fetch/clone) by acting as a git credential helper.

.DESCRIPTION
  Windows/PowerShell implementation. See README.md for details.
#>

# NOTE: Arguments are read from the automatic $args (no param block), so that
# subcommand flags like --local/--global/--host are passed through verbatim
# without PowerShell parameter binding reinterpreting them.

$ErrorActionPreference = 'Stop'
$ProgName = 'gh-autoswitch'

function Test-Windows {
    return ($IsWindows -or $env:OS -eq 'Windows_NT')
}

function Get-ConfigFile {
    if ($env:GH_AUTOSWITCH_CONFIG) { return $env:GH_AUTOSWITCH_CONFIG }
    if (Test-Windows) {
        return (Join-Path $env:APPDATA 'gh-autoswitch\config')
    }
    $base = if ($env:XDG_CONFIG_HOME) { $env:XDG_CONFIG_HOME } else { Join-Path $HOME '.config' }
    return (Join-Path $base 'gh-autoswitch/config')
}

function Get-GhConfigDir {
    if ($env:GH_CONFIG_DIR) { return $env:GH_CONFIG_DIR }
    if (Test-Windows) {
        return (Join-Path $env:APPDATA 'GitHub CLI')
    }
    $base = if ($env:XDG_CONFIG_HOME) { $env:XDG_CONFIG_HOME } else { Join-Path $HOME '.config' }
    return (Join-Path $base 'gh')
}

function Get-HostsFile {
    return (Join-Path (Get-GhConfigDir) 'hosts.yml')
}

# Resolve the mapped gh account for host/owner (exact match then "host/*").
function Resolve-Account {
    param([string]$TargetHost, [string]$Owner)

    $file = Get-ConfigFile
    if (-not (Test-Path -LiteralPath $file)) { return $null }

    $exact = "$TargetHost/$Owner"
    $wild = "$TargetHost/*"
    $exactVal = $null
    $wildVal = $null

    foreach ($raw in Get-Content -LiteralPath $file) {
        $line = $raw
        $hash = $line.IndexOf('#')
        if ($hash -ge 0) { $line = $line.Substring(0, $hash) }
        $line = $line.Trim()
        if (-not $line) { continue }
        $eq = $line.IndexOf('=')
        if ($eq -lt 0) { continue }
        $key = $line.Substring(0, $eq).Trim()
        $val = $line.Substring($eq + 1).Trim()
        if (-not $val) { continue }
        if ($key -eq $exact) { $exactVal = $val }
        elseif ($key -eq $wild) { $wildVal = $val }
    }

    if ($exactVal) { return $exactVal }
    if ($wildVal) { return $wildVal }
    return $null
}

# Read the active gh account for a host from hosts.yml (offline).
function Get-ActiveAccount {
    param([string]$TargetHost)

    $file = Get-HostsFile
    if (-not (Test-Path -LiteralPath $file)) { return $null }

    $inBlock = $false
    foreach ($raw in Get-Content -LiteralPath $file) {
        if ($raw -match '^[^\s].*:\s*$') {
            $cur = $raw -replace ':\s*$', ''
            $inBlock = ($cur -eq $TargetHost)
            continue
        }
        if ($inBlock -and $raw -match '^\s+user:\s*(\S+)\s*$') {
            return $Matches[1]
        }
    }
    return $null
}

function Switch-IfNeeded {
    param([string]$TargetHost, [string]$Account)
    $current = Get-ActiveAccount -TargetHost $TargetHost
    if ($current -eq $Account) { return }
    & gh auth switch --hostname $TargetHost --user $Account *> $null
}

function Invoke-GitCredential {
    param([string]$Operation)

    $stdin = [Console]::In.ReadToEnd()

    if ($Operation -eq 'get') {
        try {
            $targetHost = $null
            $path = $null
            foreach ($l in ($stdin -split "`n")) {
                $l = $l.TrimEnd("`r")
                if ($l.StartsWith('host=')) { $targetHost = $l.Substring(5) }
                elseif ($l.StartsWith('path=')) { $path = $l.Substring(5) }
            }
            if ($targetHost -and $path) {
                $owner = ($path -split '/')[0]
                if ($owner) {
                    $account = Resolve-Account -TargetHost $targetHost -Owner $owner
                    if ($account) {
                        Switch-IfNeeded -TargetHost $targetHost -Account $account
                    }
                }
            }
        }
        catch {
            # Fail-safe: never let switching break the git operation.
        }
    }

    # Delegate to gh so git always receives a valid credential.
    $stdin | & gh auth git-credential $Operation
    exit $LASTEXITCODE
}

function Get-ScopeArgs {
    param([string[]]$ArgList)
    $scope = '--global'
    $targetHost = 'github.com'
    for ($i = 0; $i -lt $ArgList.Count; $i++) {
        switch ($ArgList[$i]) {
            '--local'  { $scope = '--local' }
            '--global' { $scope = '--global' }
            '--host'   { $targetHost = $ArgList[$i + 1]; $i++ }
            default {
                if ($ArgList[$i] -like '--host=*') { $targetHost = $ArgList[$i].Substring(7) }
            }
        }
    }
    return @{ Scope = $scope; Host = $targetHost }
}

function Get-SelfScriptPath {
    return ($PSCommandPath -replace '\\', '/')
}

function Invoke-Install {
    param([string[]]$ArgList)
    $opts = Get-ScopeArgs -ArgList $ArgList
    $scope = $opts.Scope
    $targetHost = $opts.Host

    $script = Get-SelfScriptPath
    $helper = "!powershell -NoProfile -ExecutionPolicy Bypass -File '$script' git-credential"
    $key = "credential.https://$targetHost.helper"

    git config $scope --unset-all $key 2>$null
    git config $scope --add $key ''            # clear inherited helpers
    git config $scope --add $key $helper
    git config $scope "credential.https://$targetHost.useHttpPath" 'true'

    Write-Host "${ProgName}: installed credential helper for https://$targetHost ($scope)"
    Write-Host "  helper: $helper"
}

function Invoke-Uninstall {
    param([string[]]$ArgList)
    $opts = Get-ScopeArgs -ArgList $ArgList
    $scope = $opts.Scope
    $targetHost = $opts.Host

    git config $scope --unset-all "credential.https://$targetHost.helper" 2>$null
    git config $scope --unset "credential.https://$targetHost.useHttpPath" 2>$null
    Write-Host "${ProgName}: removed credential helper for https://$targetHost ($scope)"
}

function Invoke-Doctor {
    param([string]$TargetHost = 'github.com')
    $cfg = Get-ConfigFile
    Write-Host 'gh-autoswitch doctor'
    Write-Host "  config file : $cfg"
    if (Test-Path -LiteralPath $cfg) {
        Write-Host '  mappings    :'
        Get-Content -LiteralPath $cfg | Where-Object { $_.Trim() } | ForEach-Object { Write-Host "      $_" }
    }
    else {
        Write-Host '  mappings    : (none — file not found)'
    }
    Write-Host "  hosts.yml   : $(Get-HostsFile)"
    Write-Host "  active[$TargetHost] : $(Get-ActiveAccount -TargetHost $TargetHost)"
    Write-Host '  git helper  :'
    (git config --get-all "credential.https://$TargetHost.helper" 2>$null) | ForEach-Object { Write-Host "      $_" }
    Write-Host "  useHttpPath : $(git config --get "credential.https://$TargetHost.useHttpPath" 2>$null)"
    $gh = Get-Command gh -ErrorAction SilentlyContinue
    if ($gh) { Write-Host "  gh          : $($gh.Source)" } else { Write-Host '  gh          : NOT FOUND' }
}

function Show-Usage {
    @"
$ProgName — auto-switch gh account for git remote operations

USAGE
  $ProgName git-credential <get|store|erase>   Credential helper (called by git)
  $ProgName install   [--host H] [--local|--global]
  $ProgName uninstall [--host H] [--local|--global]
  $ProgName doctor    [host]
  $ProgName help

Config file: $(Get-ConfigFile)
  Format (one mapping per line):
    github.com/acme  = work_account
    github.com/*     = personal_account
"@ | Write-Host
}

$argv = @($args)
$Command = if ($argv.Count -ge 1) { $argv[0] } else { 'help' }
$Rest = @()
for ($i = 1; $i -lt $argv.Count; $i++) { $Rest += [string]$argv[$i] }
$Op = if ($Rest.Count -ge 1) { $Rest[0] } else { $null }

switch ($Command) {
    'git-credential' { Invoke-GitCredential -Operation $Op }
    'install'        { Invoke-Install -ArgList $Rest }
    'uninstall'      { Invoke-Uninstall -ArgList $Rest }
    'doctor'         { Invoke-Doctor -TargetHost ($(if ($Op) { $Op } else { 'github.com' })) }
    'help'           { Show-Usage }
    '-h'             { Show-Usage }
    '--help'         { Show-Usage }
    default {
        Write-Error "${ProgName}: unknown command: $Command"
        Show-Usage
        exit 2
    }
}
