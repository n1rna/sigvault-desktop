# SigVault Desktop — GPUI edition

A native rewrite of [SigVault Desktop](../sigvault-desktop) on
[GPUI](https://gpui.rs) (Zed's GPU-accelerated UI framework) instead of
Tauri + React. Same features, pages and flows; same core logic; no webview.

## Layout

```
crates/
  sigvault-core/   Wallet, hardware-signer, cloud-session, OAuth and storage logic.
                   Copied from the Tauri app's src-tauri/src with only its Tauri
                   imports re-pointed at `host` (a ~150-line shim providing
                   AppHandle / WebviewWindow / State / emit / async_runtime).
                   Every cmd_* function keeps its exact signature and behaviour.
  sigvault-app/    The GPUI front end.
    src/runtime.rs   tokio runtime + `backend(cx).call(..)` bridge to cmd_* fns
    src/state.rs     AppStore — the useAppEvents reducer; folds core events
    src/router.rs    Page enum + the backend-route/namespace rules from App.tsx
    src/shell/       root view, top bar, activity panel, update banner, toasts
    src/ui/          component kit (button, input, select, modal, QR, badges…)
    src/pages/       one module per route
```

## How it maps to the Tauri app

| Tauri / React                         | GPUI                                         |
|---------------------------------------|----------------------------------------------|
| `invoke("cmd_x", args)`               | `backend(cx).call(\|app, st\| cmd_x(app, State::new(&st), ..))` |
| `window.emit("app_event", ..)`        | `HostEvent` on a channel → `AppStore::apply` |
| `useAppEvents` / `AppStateContext`    | `state::AppStore` entity (global)            |
| react-router + backend route effect   | `router::resolve` + `shell::navigate`        |
| Tailwind tokens (`globals.css`, dark) | `theme.rs`                                   |
| `@tauri-apps/plugin-dialog` / `fs`    | `cx.prompt_for_paths` / `prompt_for_new_path` + `std::fs` |
| Tauri updater                         | release-feed check + "Download" banner       |

The data directory is the same as the Tauri build
(`<data dir>/org.sigvault.desktop`), so local wallets, settings, environment
selection and the (machine-bound, encrypted) auth token are shared between the
two apps. Set `SIGVAULT_DATA_DIR` to use a different directory.

## Building

```bash
cargo run -p sigvault-app            # debug
cargo build -p sigvault-app --release
cargo test --workspace
```

Linux needs the usual GPUI deps (Vulkan, xkbcommon, wayland/x11 dev headers)
plus `libudev-dev` for hardware wallets; no webkit2gtk.

Porting conventions for contributors: [PORTING.md](PORTING.md).
