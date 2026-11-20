# Developing SigVault Desktop

## Layout

- `crates/sigvault-core` — everything that isn't UI: local wallets (BDK),
  hardware signers (async-hwi), cloud sessions (REST + WebSocket), OAuth,
  encrypted storage. It exposes async `cmd_*` functions and reports changes as
  events. It started life as the Tauri app's `src-tauri/src`; `host.rs` stands
  in for the few Tauri types it used (`AppHandle`, `WebviewWindow`, `State`,
  `emit`), so the `cmd_*` signatures are still the Tauri command shapes.
- `crates/sigvault-app` — the GPUI app.
  - `runtime.rs` — a tokio runtime plus `backend(cx).call(..)`, the bridge to
    the core.
  - `state.rs` — `AppStore`, which folds core events into app state.
  - `router.rs` — the `Page` enum and the rules that map the core's route to a
    page.
  - `keychain.rs` — the key library (see below).
  - `shell/` — top bar, sidebar, activity bar, update banner, toasts.
  - `pages/` — one module per route; `ui/` — component kit and theme.

## Calling the core

```rust
let task = backend(cx).call(move |app, st| async move {
    cmd_x(app, State::new(&st), a, b).await
});
cx.spawn(async move |this, cx| {
    let res = task.await;                              // Result<T, String>
    this.update(cx, |this, cx| { /* apply */ cx.notify(); }).ok();
}).detach();
```

- Commands taking `window: WebviewWindow` get `app.get_webview_window("main").unwrap()`.
- Commands returning `CommandResult` → check `.success` / `.message` / `.data`.
- Request DTOs are the core's own structs; most derive only `Deserialize`, so
  build them with `serde_json::from_value(json!({...}))` when fields are private.
- Never block the UI thread, and never call core functions outside
  `backend(cx).call`.

## Events and navigation

Core events land in `state::AppStore` (`crate::state::store(cx)`). Subscribe
with `cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| ...)` or
`cx.observe(...)`. After creating, deleting, renaming, locking or unlocking a
wallet, call `crate::state::local_changed(cx)` so the sidebar refreshes.

Navigate with `crate::shell::navigate(Page::LocalWallet(id), cx)`; pages are
rebuilt on navigation. `crate::shell::toast(cx, "Copied")` shows a toast.

## Design system

The app mirrors the SigVault web dashboard.

- **Theme** (`theme.rs`): the dark "Instrument" ladder — `page`, `canvas`,
  `surface`, `surface_2`, `surface_3`, `inset`, `hover_strong`; ink `ink` …
  `ink_4`; lines `line` … `line_tertiary`; `brand` / `brand_ink`. Prefer these
  tokens in new code.
- **Type**: Manrope for UI (`FONT_SANS`), Space Mono for numbers and eyebrows
  (`FONT_MONO`), both bundled in `assets/fonts` (OFL). Radii: `RADIUS_CHIP` 8,
  `RADIUS_CONTROL` 10, `RADIUS_CARD` 12, `RADIUS_PANEL` 16.
- **Shell**: a full-width top bar
  (`sigvault / <workspace> [pill] ⇕ / <wallet> ⇕ … network | version`). The
  workspace switcher covers "On this device" and "SigVault Cloud" (⌘1 / ⌘2) —
  never call them "local mode" / "cloud mode" in the UI. The sidebar sits on
  the canvas; content is an inset surface panel.
- **Pages** always render `crate::ui::page_frame(id)` — never their own outer
  container, padding or max width.
  - Header: `.crumb(..)` for drill-down (not back buttons), `.title(..)`,
    `.subtitle(..)`, `.title_adornment(badge)`, `.action(..)` with the primary
    CTA last.
  - Wizards: `.header_extra(stepper(..))` and
    `.footer(footer_bar(left, right))` — Back on the left, Cancel / Continue
    on the right. Single-column forms go in `div().max_w(FORM_WIDTH)`.
  - Group content with `section("Title")`; lists are one bordered `card()`
    with `divider()` rows; destructive confirmations go in `modal(id, width)`.
- **Components** (`crate::ui`): `button`, `icon_button`, `icon(name)` (lucide
  SVGs in `assets/icons`), `badge`, `pill`, `card`, `alert`, `field`,
  `checkbox`, `choice_card`, `tabs`, `copy_field`, `empty_state`, `stepper`,
  `stat_grid`, `section_head`, `modal`, `QrCode`, `Select`, `TextInput`
  (`InputEvent::{Changed, Submit}`), and the `format_*` helpers.

## Key library

`crate::keychain::keychain(cx)` holds the user's keys: `keys()`,
`keys_for_network`, `get`, `find_match`, `add_public(NewKey)`,
`add_hot(NewKey, HotSecret) -> Task`, `secret(id) -> Task<HotSecret>`,
`rename`, `delete`, `is_unlocked` / `has_password` / `unlock` / `lock`; it
emits `KeychainEvent`. Private keys are encrypted with the core's seed envelope
under one keychain password. Hot secrets need the keychain unlocked: show
`pages/local/keys/unlock.rs::KeychainUnlock` in a `modal` and retry on
`UnlockEvent::Unlocked`. `pages/local/keys/add_flow.rs::AddKeyFlow` is the
embeddable add/import flow.

## GPUI notes (gpui 0.2.2)

- Scrolling containers need `.id(..)` and `.overflow_y_scroll()`.
- Flex children with long text need `.min_w_0()` (plus `.flex_basis(px(0.))`
  for equal columns).
- Every interactive element needs an `.id(..)` unique within its parent.
- Create `TextInput` entities in `new(window, cx)`, keep them as fields, and
  subscribe to their `InputEvent`s.
- File dialogs: `cx.prompt_for_paths(..)` / `cx.prompt_for_new_path(..)`;
  clipboard: `cx.write_to_clipboard(..)`; links: `cx.open_url(..)`.
- There is no letter-spacing support.

## Debug hooks

- `SIGVAULT_DATA_DIR=<dir>` — use another data directory (any build). Handy
  for a throwaway regtest setup.
- `SIGVAULT_NO_UPDATE_CHECK=1` — skip the release-feed check.

Debug builds only:

- `SIGVAULT_DEV_PAGE=<path>` — open straight on a page, e.g. `/local/keys`,
  `/local/wallets/new`, `/cloud/sessions`.
- `SIGVAULT_DEV_CLOUD_FIXTURE=signed_out|signed_out_login|signed_in|session_register|session_sign`
  (with `SIGVAULT_DEV_CLOUD_STEP`) — render cloud pages from fixtures, no
  account needed.
- `SIGVAULT_DEV_WIZARD`, `SIGVAULT_DEV_DESCRIPTOR`, `SIGVAULT_DEV_KEY_SOURCE`,
  `SIGVAULT_DEV_KEY_TEXT` — jump into wallet-creation, restore and add-key
  steps with prefilled input.
- `SIGVAULT_E2E_SIGNER_MNEMONIC`, `SIGVAULT_E2E_AUTH_URL_FILE` — software signer
  and OAuth redirect capture for end-to-end tests.
- `SIGVAULT_E2E_DRIVER=127.0.0.1:<port>` — the e2e driver (below).

## End-to-end tests

GPUI has no DOM or accessibility tree, so tests drive the app through a small
driver built into debug builds (`src/e2e.rs`). With `SIGVAULT_E2E_DRIVER` set,
the app takes newline-delimited JSON on that address:

- `{"cmd":"snapshot"}` → the page path plus every reported widget:
  `{role, label, enabled, visible, value?, selected?, focused?, bounds}`.
- `{"cmd":"click","role":"button","label":"Continue","exact":true}` — runs
  the widget's click handler; refused when it is disabled or covered by a
  modal or menu.
- `{"cmd":"type","text":"…"}` / `{"cmd":"key","keys":"enter"}` — keystrokes
  through `Window::dispatch_keystroke`, as real typing.
- `{"cmd":"quit"}`.

Widgets report themselves while the frame is laid out: the kit's `button`,
`checkbox`, `tabs`, `alert`, `badge`, `Select`, `TextInput`, modals, page
titles and crumbs already do, as do the sidebar, welcome cards, network rows
and hardware-device rows. When you add a custom clickable element a test
needs, use `.clickable(Probe::new("role", "Label"), handler)` (from
`crate::e2e::E2eClick`) instead of `.on_click(handler)`, or `.e2e(Probe…)` for
something that is only read. Normal runs skip all of it (no env var → no
probes, no socket).

- `crates/sigvault-app/tests/ui_e2e.rs` — offline UI flows, run in CI under
  Xvfb: `cargo test -p sigvault-app --test ui_e2e -- --ignored --test-threads=1`
  (`xvfb-run -a` in front on a headless Linux box).
- The sigvault repo's `e2e/specs/desktop` runs the cloud flows (OIDC sign-in,
  device registration and signing on an emulated BitBox02) against regtest,
  using the `nightly-debug.yaml` artifact and the same driver
  (`e2e/fixtures/desktop.ts`).

## Checks

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --no-deps   # warnings are errors
cargo test --workspace
cargo test -p sigvault-app --test ui_e2e -- --ignored --test-threads=1   # opens windows
```

Look at every page you change in the running app before calling it done.
