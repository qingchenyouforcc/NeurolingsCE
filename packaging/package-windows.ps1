# Assembles a Windows release folder for Neurolings-rs.
# Run after: cargo build --release ; flutter build windows --release
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$out  = Join-Path $root 'dist\NeurolingsCE-windows-x64'

if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Copy-IfExists($src, $dest) {
    if (Test-Path $src) { Copy-Item $src $dest -Force; Write-Host "copied: $src" }
    else { Write-Warning "missing: $src" }
}

# Runtime + CLI (Rust release binaries)
Copy-IfExists (Join-Path $root 'target\release\NeurolingsCE.exe')     $out
Copy-IfExists (Join-Path $root 'target\release\NeurolingsCE-cli.exe') $out

# Flutter manager (entire runner folder)
$manager = Join-Path $root 'manager\build\windows\x64\runner\Release'
if (Test-Path $manager) {
    Copy-Item (Join-Path $manager '*') $out -Recurse -Force
    Write-Host "copied: manager runner"
} else {
    Write-Warning "missing: $manager (run: flutter build windows --release)"
}

# Generate SHA256SUMS.txt over all files in the output folder
$sums = Join-Path $out 'SHA256SUMS.txt'
Get-ChildItem $out -File | Where-Object { $_.Name -ne 'SHA256SUMS.txt' } | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    "$hash  $($_.Name)" | Out-File $sums -Append -Encoding ascii
}
Write-Host "wrote: $sums"
Write-Host "Release folder ready: $out"
