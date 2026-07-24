<#
  Thin wrapper: configure git to use gh-autoswitch as the credential helper.
  Usage: .\install.ps1 [--host github.com] [--local|--global]

  Arguments come from the automatic $args so flags pass through verbatim.
#>
& (Join-Path $PSScriptRoot 'bin\gh-autoswitch.ps1') install @args
