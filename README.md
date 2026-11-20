<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/sigvault-horizontal-dark.png">
    <img alt="SigVault" src="docs/assets/sigvault-horizontal.png" width="420">
  </picture>
</p>

# SigVault Desktop

[![License](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/n1rna/sigvault-desktop?color=blue&label=release)](https://github.com/n1rna/sigvault-desktop/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/n1rna/sigvault-desktop/ci.yaml?branch=main&label=CI)](https://github.com/n1rna/sigvault-desktop/actions/workflows/ci.yaml)

A native Bitcoin wallet for single-key, multisig and time-locked vaults. Keep
wallets and keys on your own computer, or join your team's signing sessions on
[SigVault](https://sigvault.org) — with hardware signing devices either way.

The app is written in Rust on [GPUI](https://gpui.rs), the GPU-accelerated UI
framework behind the Zed editor. There is no web view.

## Two workspaces

Switch with the workspace menu in the top bar, or ⌘1 / ⌘2 (Ctrl on Windows and Linux).

- **On this device** — no account needed. Build a key library (generate keys,
  import recovery phrases or xpubs, connect hardware wallets), then combine
  those keys into wallets: single key, M-of-N multisig, or a key with
  time-locked recovery paths. Receive, send, and restore wallets from a backup
  file or descriptor. Private keys are encrypted at rest under your keychain
  password.
- **SigVault Cloud** — sign in to a SigVault deployment and take part in remote
  sessions: register a device's key for a team wallet, or review and sign a
  transaction on your hardware wallet. Partial signatures are relayed through
  the server; SigVault never holds your keys.

## Supported platforms

| Platform | Architecture | Formats |
|----------|-------------|---------|
| macOS | Apple Silicon, Intel | `.dmg` |
| Linux | x86_64 | `.deb`, `.AppImage` |
| Windows | x86_64 | `-setup.exe` |

## Supported hardware wallets

- **Trezor** — all models
- **Ledger** — Nano S, Nano X, Nano S Plus
- **BitBox02** — Multi edition
- **Blockstream Jade**
- **Coldcard**
- **Specter DIY**

## Building from source

Prerequisites: [Rust](https://rustup.rs/) (stable). On Linux (Ubuntu/Debian):

```bash
sudo apt-get install -y pkg-config libssl-dev libudev-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev \
  libfontconfig1-dev libfreetype6-dev libvulkan1
```

```bash
git clone https://github.com/n1rna/sigvault-desktop.git
cd sigvault-desktop
cargo run -p sigvault-app          # debug build
cargo test --workspace
```

No `.env` is needed. The app fetches the list of deployments from
[`sigvault.org/environments.json`](https://sigvault.org/environments.json) at
boot and lets you pick one when signing in; the OAuth client for each is built
into the binary (`BUILTIN_OAUTH_CLIENTS` in
`crates/sigvault-core/src/env_config/mod.rs`).

### Installers

```bash
cargo install --locked cargo-packager
cargo build --release -p sigvault-app
cargo packager -p sigvault-app --release          # .app/.dmg, .deb/.AppImage or -setup.exe
```

Output lands in `target/release/`. Release CI also signs the update archives;
see `.env.example`.

## Project structure

```
crates/
  sigvault-core/   Wallets, hardware signers, cloud sessions, OAuth, storage.
                   UI-free; exposes async cmd_* functions and emits events.
  sigvault-app/    The GPUI app: shell, pages, UI kit, key library, PSBT tools.
packaging/icons/   App icons used by cargo-packager.
docs/              Development notes and QA checklists.
```

[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) covers how the app talks to the
core, the design system and the debug hooks.

## Data

Everything lives in the per-user data directory under `org.sigvault.desktop`
(`~/Library/Application Support/` on macOS, `~/.local/share/` on Linux,
`%APPDATA%` on Windows) — the same directory the earlier Tauri-based releases
used, so wallets and settings carry over.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security issues: [SECURITY.md](SECURITY.md).
