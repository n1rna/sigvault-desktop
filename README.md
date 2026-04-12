# SigVault Desktop

A Bitcoin multisig wallet manager built with [Tauri 2](https://v2.tauri.app), React, and Rust. SigVault Desktop connects to hardware signing devices (Trezor, BitBox) and coordinates multisig transaction signing through a WebSocket-based remote session protocol.

## Tech Stack

- **Frontend** — React 19, TypeScript, Tailwind CSS 4, Vite
- **Backend** — Rust (Tauri 2), async runtime via Tokio
- **Hardware wallets** — Trezor and BitBox support via [async-hwi](https://github.com/n1rna/async-hwi)
- **Security** — Tauri Stronghold for secure local storage
- **Auth** — OAuth 2.0 (ZITADEL)

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

2. **Set up environment variables**

   Copy the example env file and adjust values for your environment:

   ```bash
   cp .env.example .env
   ```

   If you have access to the `ee` CLI, you can hydrate directly:

   ```bash
   ee hydrate development -c .ee.desktop -o .env
   ```

3. **Install frontend dependencies**

   ```bash
   bun install
   ```

4. **Run in development mode**

   ```bash
   bun run tauri dev
   ```

## Building for Production

```bash
bun run tauri build
```

Build artifacts are placed in `src-tauri/target/release/bundle/` and include `.deb`, `.rpm`, and `.AppImage` packages for Linux.

## Project Structure

```
sigvault-desktop/
├── src/                  # React frontend
│   ├── components/       # UI components (device cards, navbar, signing flows)
│   ├── pages/            # Route pages (dashboard, login, sessions)
│   ├── contexts/         # React context providers
│   └── hooks/            # Custom hooks
├── src-tauri/            # Rust backend (Tauri)
│   ├── src/              # Rust source (commands, device integration, WebSocket)
│   └── Cargo.toml        # Rust dependencies
├── .github/workflows/    # CI/CD (release automation)
└── package.json          # Frontend dependencies & scripts
```

## Releasing

Releases are automated via GitHub Actions. To create a new release:

1. Tag the commit: `git tag v0.x.y && git push origin v0.x.y`
2. Trigger the workflow: `gh workflow run release.yaml -f tag=v0.x.y`

The workflow builds Linux packages, generates SHA-256 checksums, and publishes a GitHub Release.

## IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
