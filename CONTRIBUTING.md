# Contributing to SigVault Desktop

Thank you for your interest in contributing to SigVault Desktop! This guide will help you get started.

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Bun](https://bun.sh/) (or Node.js 22+)
- Linux system dependencies:

  ```bash
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libssl-dev \
    libudev-dev
  ```

### Getting Started

1. Fork and clone the repository
2. Install dependencies: `bun install`
3. Start development: `bun run tauri dev`

No `.env` or secrets are needed to run the app — pick a deployment (regtest,
signet, testnet) on the login screen. Only signed release builds need the
Tauri updater signing key; see `.env.example`.

### Running Tests

```bash
# Frontend tests
bun test

# Rust tests
cd src-tauri && cargo test
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
3. Ensure all tests pass (`bun test` and `cargo test`)
4. Run linting: `cargo clippy` for Rust
5. Open a PR with a clear description of changes
6. Address any review feedback

## Project Structure

```
sigvault-desktop/
├── src/                  # React/TypeScript frontend
│   ├── components/       # Reusable UI components
│   ├── pages/            # Route pages
│   ├── hooks/            # Custom React hooks
│   ├── contexts/         # React context providers
│   └── types/            # TypeScript type definitions
├── src-tauri/            # Rust backend (Tauri 2)
│   └── src/
│       ├── api/          # HTTP API client
│       ├── commands/     # Tauri command handlers
│       ├── hwi/          # Hardware wallet integration
│       ├── oauth/        # OAuth 2.0 authentication
│       ├── state/        # Application state management
│       ├── storage/      # Secure local storage
│       ├── websocket/    # WebSocket session handling
│       └── window/       # Window event management
└── .github/workflows/    # CI/CD automation
```

## Hardware Wallet Development

SigVault integrates with hardware signing devices via [async-hwi](https://github.com/n1rna/async-hwi). If you're working on hardware wallet features, review `src-tauri/src/hwi/TREZOR_LIMITATIONS.md` for known constraints.

Testing hardware wallet code requires physical devices. Integration tests in `src-tauri/tests/` are marked `#[ignore]` and must be run manually with devices connected.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

If you discover a security vulnerability, please follow the process in [SECURITY.md](SECURITY.md). Do not open a public issue.
