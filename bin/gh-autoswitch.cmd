@echo off
REM gh-autoswitch — PATH shim that forwards to the PowerShell implementation.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0gh-autoswitch.ps1" %*
