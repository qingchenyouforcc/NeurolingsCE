# Neurolings-rs packaging

Build/release tooling for the Rust + Flutter rewrite.

## package-windows.ps1

Assembles a Windows release folder from the built artifacts:

- `target/release/NeurolingsCE.exe` (runtime)
- `target/release/NeurolingsCE-cli.exe` (CLI)
- `manager/build/windows/x64/runner/Release/` (Flutter manager)
- generates `SHA256SUMS.txt`

Usage:

```powershell
cargo build --release
flutter build windows --release
powershell -ExecutionPolicy Bypass -File packaging/package-windows.ps1
```

## Update manifest

The release pipeline publishes a static manifest matching
`updater-schema/latest.example.json` (see the original NeurolingsCE repo).
The Rust `neurolings-store::updater` module fetches it, decides whether the
running version should update (including mandatory updates below
`min_supported_version`), and verifies the downloaded asset's SHA-256 before
installation.
