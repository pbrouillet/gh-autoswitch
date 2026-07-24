#Requires -Modules Pester
<#
  Pester tests for the gh-autoswitch PowerShell credential helper.
  A mock `gh` (gh.cmd) is placed on PATH so no real GitHub auth is touched.

  Run:  Invoke-Pester -Path test\gh-autoswitch.Tests.ps1
#>

BeforeAll {
    $script:Root = Split-Path -Parent $PSScriptRoot
    $script:Ghas = Join-Path $Root 'bin\gh-autoswitch.ps1'

    $script:Work = Join-Path ([IO.Path]::GetTempPath()) ("ghas_" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $Work | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Work 'gh') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Work 'bin') | Out-Null

    $env:GH_CONFIG_DIR = Join-Path $Work 'gh'
    $env:GH_AUTOSWITCH_CONFIG = Join-Path $Work 'config'
    $env:MOCK_LOG = Join-Path $Work 'gh.log'

    Set-Content -LiteralPath $env:GH_AUTOSWITCH_CONFIG -Encoding ascii -Value @(
        '# comment',
        'github.com/acme-corp = alice_work',
        'github.com/*         = alice_personal'
    )

    # Mock gh as a .cmd that dispatches to an embedded PowerShell mock.
    $mockPs = Join-Path $Work 'bin\ghmock.ps1'
    Set-Content -LiteralPath $mockPs -Encoding ascii -Value @'
param([Parameter(ValueFromRemainingArguments=$true)][string[]]$a)
$log = $env:MOCK_LOG
$dir = $env:GH_CONFIG_DIR
if ($a[0] -eq 'auth' -and $a[1] -eq 'switch') {
    $host2=''; $user=''
    for ($i=2; $i -lt $a.Count; $i++) {
        if ($a[$i] -eq '--hostname') { $host2=$a[$i+1]; $i++ }
        elseif ($a[$i] -eq '--user') { $user=$a[$i+1]; $i++ }
    }
    Add-Content -LiteralPath $log -Value "switch $host2 $user"
    Set-Content -LiteralPath (Join-Path $dir 'hosts.yml') -Value @(
        "${host2}:","    git_protocol: https","    users:","        alice_work:","        alice_personal:","    user: $user")
    exit 0
}
if ($a[0] -eq 'auth' -and $a[1] -eq 'git-credential') {
    $op = $a[2]
    [Console]::In.ReadToEnd() | Out-Null
    Add-Content -LiteralPath $log -Value "gitcred $op"
    if ($op -eq 'get') {
        Write-Output "protocol=https"
        Write-Output "host=github.com"
        Write-Output "username=x-access-token"
        Write-Output "password=TOKEN"
    }
    exit 0
}
[Console]::Error.WriteLine("unexpected gh args: $($a -join ' ')")
exit 99
'@
    $mockCmd = Join-Path $Work 'bin\gh.cmd'
    Set-Content -LiteralPath $mockCmd -Encoding ascii -Value @(
        '@echo off',
        'powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0ghmock.ps1" %*')

    $env:PATH = (Join-Path $Work 'bin') + [IO.Path]::PathSeparator + $env:PATH

    function Set-Hosts([string]$active) {
        Set-Content -LiteralPath (Join-Path $env:GH_CONFIG_DIR 'hosts.yml') -Value @(
            'github.com:', '    git_protocol: https', '    users:',
            '        alice_work:', '        alice_personal:', "    user: $active")
    }

    function Invoke-Get([string]$owner) {
        $inp = "protocol=https`nhost=github.com`npath=$owner/repo.git`n`n"
        return ($inp | powershell -NoProfile -ExecutionPolicy Bypass -File $Ghas git-credential get)
    }
}

AfterAll {
    Remove-Item -Recurse -Force -LiteralPath $script:Work -ErrorAction SilentlyContinue
}

Describe 'gh-autoswitch credential helper (PowerShell)' {

    It 'exact match: switches to alice_work and returns a token' {
        Set-Hosts 'alice_personal'; Set-Content -LiteralPath $env:MOCK_LOG -Value ''
        $out = Invoke-Get 'acme-corp'
        ($out -join "`n") | Should -Match 'password=TOKEN'
        (Get-Content -Raw $env:MOCK_LOG) | Should -Match 'switch github.com alice_work'
    }

    It 'already-active: does not switch but still delegates' {
        Set-Hosts 'alice_work'; Set-Content -LiteralPath $env:MOCK_LOG -Value ''
        Invoke-Get 'acme-corp' | Out-Null
        (Get-Content -Raw $env:MOCK_LOG) | Should -Not -Match 'switch '
        (Get-Content -Raw $env:MOCK_LOG) | Should -Match 'gitcred get'
    }

    It 'wildcard: unknown owner falls back to alice_personal' {
        Set-Hosts 'alice_work'; Set-Content -LiteralPath $env:MOCK_LOG -Value ''
        Invoke-Get 'someoneelse' | Out-Null
        (Get-Content -Raw $env:MOCK_LOG) | Should -Match 'switch github.com alice_personal'
    }

    It 'no matching host: does not switch' {
        Set-Hosts 'alice_work'; Set-Content -LiteralPath $env:MOCK_LOG -Value ''
        $inp = "protocol=https`nhost=other.example.com`npath=x/y.git`n`n"
        $inp | powershell -NoProfile -ExecutionPolicy Bypass -File $Ghas git-credential get | Out-Null
        (Get-Content -Raw $env:MOCK_LOG) | Should -Not -Match 'switch '
    }

    It 'store: passes through without switching' {
        Set-Hosts 'alice_personal'; Set-Content -LiteralPath $env:MOCK_LOG -Value ''
        $inp = "protocol=https`nhost=github.com`npath=acme-corp/repo.git`nusername=u`npassword=p`n`n"
        $inp | powershell -NoProfile -ExecutionPolicy Bypass -File $Ghas git-credential store | Out-Null
        (Get-Content -Raw $env:MOCK_LOG) | Should -Not -Match 'switch '
        (Get-Content -Raw $env:MOCK_LOG) | Should -Match 'gitcred store'
    }
}
