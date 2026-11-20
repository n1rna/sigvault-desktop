# Contributing to SigVault Desktop

Thank you for your interest in contributing to SigVault Desktop! This guide will help you get started.

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- Linux system dependencies:

  ```bash
  sudo apt-get install -y pkg-config libssl-dev libudev-dev \
    libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev \
    libfontconfig1-dev libfreetype6-dev libvulkan1
  ```

### Getting Started

1. Fork and clone the repository
2. Run the app: `cargo run -p sigvault-app`

No `.env` or secrets are needed to run the app — pick a deployment (regtest,
signet, testnet) when signing in. Only signed release builds need the updater
signing key; see `.env.example`.

[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) explains how the app is put
together, the design system, and the debug-only hooks for opening a page
directly or using a throwaway data directory.

### Running Tests

```bash
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps   # warnings are errors
```

## Making Changes

### Branch Naming

- `feat/description` — new features
- `fix/description` — bug fixes
- `docs/description` — documentation
- `refactor/description` — code refactoring

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add BitBox02 device support
fix: handle disconnected device during signing
docs: update hardware wallet compatibility table
```

### Pull Request Process

1. Create a feature branch from `main`
2. Make your changes with clear, focused commits
3. Ensure all tests pass (`cargo test --workspace`)
4. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets`
5. Open a PR with a clear description of changes
6. Address any review feedback

## Project Structure

```
sigvault-desktop/
├── crates/
│   ├── sigvault-core/        # UI-free core
│   │   ├── src/
│   │   │   ├── api/          # HTTP API client
│   │   │   ├── commands/     # cmd_* entry points the app calls
│   │   │   ├── hwi/          # Hardware wallet integration
│   │   │   ├── local_wallet/ # On-device wallets (BDK)
│   │   │   ├── oauth/        # OAuth 2.0 authentication
│   │   │   ├── state/        # Application state
│   │   │   ├── storage/      # Encrypted local storage
│   │   │   ├── websocket/    # Remote session connection
│   │   │   └── host.rs       # App handle, events and data dir
│   │   └── tests/            # Hardware-wallet and regtest integration tests
│   └── sigvault-app/         # GPUI app
│       ├── src/shell/        # Top bar, sidebar, banners, toasts
│       ├── src/pages/        # One module per route
│       ├── src/ui/           # Component kit and theme
│       └── assets/           # Fonts and icons
├── packaging/icons/
└── .github/workflows/        # CI and release
```

## Hardware Wallet Development

SigVault integrates with hardware signing devices via [async-hwi](https://github.com/n1rna/async-hwi). If you're working on hardware wallet features, review `crates/sigvault-core/src/hwi/TREZOR_LIMITATIONS.md` for known constraints.

Testing hardware wallet code requires physical devices. Integration tests in `crates/sigvault-core/tests/` are marked `#[ignore]` and must be run manually with devices connected (`crates/sigvault-core/tests/run_hwi_tests.sh`).

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

If you discover a security vulnerability, please follow the process in [SECURITY.md](SECURITY.md). Do not open a public issue.
