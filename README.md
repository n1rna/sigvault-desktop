# SigVault Desktop

A Bitcoin multisig wallet manager that connects to hardware signing devices and coordinates multisig transaction signing through remote sessions.

## Supported Platforms

| Platform | Architecture | Formats |
|----------|-------------|---------|
| Linux | x86_64 | `.deb`, `.rpm`, `.AppImage` |
| macOS | Apple Silicon (ARM) | `.dmg` |
| macOS | Intel (x86_64) | `.dmg` |
| Windows | x86_64 | `.msi`, `.exe` |

## Supported Hardware Wallets

- **Trezor** — all models (Model One, Model T, Safe series)
- **BitBox02** — Multi edition
- **Blockstream Jade** — including QEMU emulator for development

## Remote Sessions

SigVault Desktop connects to the [SigVault](https://sigvault.io) coordination server over WebSocket to participate in remote signing sessions. A session brings together multiple signers — each running the desktop app with their own hardware wallet — to collaboratively sign Bitcoin transactions.

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

2. **Set up environment variables**

   Copy the example env file and adjust values for your environment:

   ```bash
   cp .env.example .env
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
