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

A Bitcoin multisig wallet manager that connects to hardware signing devices and coordinates multisig transaction signing through remote sessions.

## Supported Platforms

| Platform | Architecture | Formats |
|----------|-------------|---------|
| Linux | x86_64 | `.deb`, `.rpm`, `.AppImage` |
| macOS | Apple Silicon (ARM) | `.dmg` |
| macOS | Intel (x86_64) | `.dmg` |
| Windows | x86_64 | `.msi`, `.exe` |

## Supported Hardware Wallets

- **Trezor** — all models
- **Ledger** — Nano S, Nano X, Nano S Plus
- **BitBox02** — Multi edition
- **Blockstream Jade**
- **Coldcard**
- **Specter DIY**

## Remote Sessions

SigVault Desktop connects to the [SigVault](https://sigvault.org) coordination server over WebSocket to participate in remote signing sessions. A session brings together multiple signers — each running the desktop app with their own hardware wallet — to collaboratively sign Bitcoin transactions.

Session workflow:

1. A session is created on the SigVault web app with a transaction to sign
2. Each signer opens SigVault Desktop, connects their hardware wallet, and joins the session
3. The app presents the transaction details for review on the hardware device
4. Each signer approves and signs on their device — partial signatures are relayed through the server
5. Once the required threshold of signatures is reached, the fully signed transaction is broadcast

The app includes built-in auto-update support. When a new release is available, a banner appears with a one-click update option.

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Bun](https://bun.sh/) (or Node.js 22+)
- Linux system dependencies (Ubuntu/Debian):

  ```bash
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libssl-dev \
    libudev-dev
  ```

## Getting Started

1. **Clone the repo**

   ```bash
   git clone https://github.com/n1rna/sigvault-desktop.git
   cd sigvault-desktop
   ```

2. **Install frontend dependencies**

   ```bash
   bun install
   ```

3. **Run in development mode**

   ```bash
   bun run tauri dev
   ```

No `.env` is needed. The app fetches the list of available deployments from
[`sigvault.org/environments.json`](https://sigvault.org/environments.json) at
boot and lets you pick one on the login screen; the OAuth client ID and token
URL for each are built into the binary (`BUILTIN_OAUTH_CLIENTS` in
`src-tauri/src/env_config/mod.rs`), so a dev build can sign in to regtest,
signet, or testnet without rebuilding.

## Building for Production

```bash
bun run tauri build
```

To produce *signed* updater artifacts you also need the Tauri signing key
exported in your shell — see `.env.example`. Release CI takes it from the
`ENV_VARS_DESKTOP` secret, which the private `sigvault-secrets` repo owns.

Build artifacts are placed in `src-tauri/target/release/bundle/`.

## Running Tests

```bash
bun run test
```

## Project Structure

```
sigvault-desktop/
├── src/                  # React frontend
│   ├── components/       # UI components (device cards, navbar, signing flows)
│   ├── pages/            # Route pages (dashboard, login, sessions)
│   ├── contexts/         # React context providers
│   └── hooks/            # Custom hooks
├── src-tauri/            # Rust backend (Tauri)
│   ├── src/              # Commands, device integration, WebSocket client
│   └── Cargo.toml        # Rust dependencies
├── .github/workflows/    # CI/CD (release automation)
└── package.json          # Frontend dependencies & scripts
```
