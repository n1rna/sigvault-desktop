# Porting conventions (Tauri/React → GPUI)

Source of truth for behaviour: `../sigvault-desktop/src` (React) and
`../sigvault-desktop/src-tauri/src` (Rust). The Rust core is copied into
`crates/sigvault-core` **unchanged** except for Tauri imports, which now point at
`sigvault_core::host` (a shim with `AppHandle`, `WebviewWindow`, `State`).

## Calling backend commands
`invoke("cmd_x", {a, b})` in TSX becomes:

```rust
let task = backend(cx).call(move |app, st| async move {
    cmd_x(app, State::new(&st), a, b).await          // same signature as the Tauri command
});
cx.spawn(async move |this, cx| {
    let res = task.await;                              // Result<T, String>
    this.update(cx, |this, cx| { /* apply */ cx.notify(); }).ok();
}).detach();
```
- Commands taking `window: WebviewWindow` get `app.get_webview_window("main").unwrap()`.
- Commands returning `CommandResult` → check `.success` / `.message` / `.data` (serde_json::Value).
- Request DTOs are the core's own structs (`sigvault_core::local_wallet::commands::*Request`);
  most derive only `Deserialize`, so build them with `serde_json::from_value(json!({...}))`
  when fields/constructors are private, or construct directly when fields are pub.
- Never block the UI thread; never call core fns outside `backend(cx).call`.

## Events
Backend `emit`s land in `state::AppStore` (`crate::state::store(cx)`), mirroring
`useAppEvents`. Subscribe with `cx.subscribe(&store(cx), |this, _, ev: &StoreEvent, cx| ...)`
or `cx.observe(...)`. Sync progress: `StoreEvent::SyncProgress` / `store.sync_progress`.

## Navigation
`crate::shell::navigate(Page::LocalWallet(id), cx)` (see `router.rs`). Pages are
rebuilt on navigation. `crate::shell::toast(cx, "Copied")` for transient toasts.

## UI kit (`crate::ui`)
`button(id,label)` (.outline/.ghost/.secondary/.destructive/.link/.small/.large/.icon/.icon_right/
.loading/.disabled/.full_width/.on_click), `icon_button`, `icon(name)` (assets/icons/*.svg — add
new lucide SVGs there, stroke="black", copy path data from the TSX), `spinner`, `spinner_with`,
`badge(label, Tone)`, `dot`, `card()`, `alert(Tone, msg)`, `title/heading/muted/mono/label/eyebrow`,
`field(label, control, hint)`, `divider`, `kv_row`, `checkbox`, `choice_card`, `tabs`,
`copy_field`, `empty_state`, `page_scroll(id, max_w)`, `back_link`, `stepper`, `modal(id, w)`,
`QrCode::new(data, size)`, `Select` entity, `TextInput` entity (`.placeholder/.masked/.mono/
.multiline(rows)/.with_text`, `.text()`, `.set_text`, `InputEvent::{Changed,Submit}`),
`format_btc/format_sat/truncate_middle/format_timestamp`. Theme: `crate::theme::{theme, alpha, FONT_MONO}`.
Tailwind `text-[13px]` → `.text_size(px(13.))`; `bg-primary/10` → `.bg(alpha(t.primary, 0.1))`.

## GPUI gotchas (gpui 0.2.2, source in /private/tmp/claude-501/gpui-0.2.2)
- Scrolling containers need `.id(..)` + `.overflow_y_scroll()`.
- Flex children with long text: add `.min_w_0()` (and `.flex_basis(px(0.))` for equal columns).
- Every interactive element needs a unique `.id(..)` within its parent.
- `cx.listener(|this, ev, window, cx| ...)` for handlers; `.when(cond, |d| ...)`, `.map(...)`.
- TextInput entities must be created in `new(window, cx)` via `cx.new(|cx| TextInput::new(window, cx))`
  and kept as fields; subscribe with `cx.subscribe(&input, |this, _, ev: &InputEvent, cx| ...)`.
- File dialogs: `cx.prompt_for_paths(PathPromptOptions{..})` / `cx.prompt_for_new_path(dir, name)`;
  clipboard: `cx.write_to_clipboard(ClipboardItem::new_string(..))`; urls: `cx.open_url(..)`.

## Verifying
`cargo build -p sigvault-app` must stay warning-free in your files.
Screenshot helper: `/private/tmp/claude-501/tools/shot.sh <out.png> [wait_secs]` launches the
debug build with `SIGVAULT_DATA_DIR` (default scratch dir) and captures the window; read the PNG.

# Local mode design system (v2)

Local mode is a desktop app: persistent left sidebar (`shell/sidebar.rs`: wallets,
Restore wallet, Key library, Add key, keychain lock, Settings) + one page frame.

**Every local page renders `crate::ui::page_frame(id)`** (ui/frame.rs) — never its own
outer container, padding, max width, or centered column. That guarantees identical
header position, padding (32px) and content width (max 1040) everywhere.
- Header: `.crumb("Wallets", Some(Page::LocalWallets)).crumb("Savings", None)` for
  drill-down pages (instead of back buttons), `.title(..)`, `.subtitle(..)`,
  `.title_adornment(badge)`, `.action(..)` (primary CTA last, `button(..).small()` sizes
  only — use default size for the primary CTA).
- Wizards / multi-step flows: `.header_extra(stepper(..))` + `.footer(footer_bar(left, right))`
  with Back on the left and Cancel/Continue on the right. Step content goes in the body;
  single-column forms are wrapped in `div().max_w(FORM_WIDTH)` (left-aligned, not centered).
- Group content with `section("Title").description(..).action(..)`.
- Lists: bordered `card()` containing rows separated by `divider()` (row height ~56px,
  px_4, hover bg `alpha(t.muted, 0.5)`), not a stack of separate floating cards.
- Dialogs: `crate::ui::modal(id, width)`. Destructive confirm in a modal.
- Typography: page title 20px semibold (frame), section title 13px semibold, body 13px,
  meta 12px muted, mono 12px. Don't invent other sizes.
- Do not render Settings/Refresh/Back icon buttons that duplicate the sidebar.
- After creating/deleting/renaming/locking/unlocking a wallet call
  `crate::state::local_changed(cx)` so the sidebar refreshes.

**Key library** (`crate::keychain`): `keychain(cx)` entity — `keys()`, `keys_for_network`,
`get`, `find_match`, `add_public(NewKey)`, `add_hot(NewKey, HotSecret) -> Task`,
`secret(id) -> Task<HotSecret>`, `rename`, `delete`, `is_unlocked/has_password/unlock/lock`,
emits `KeychainEvent`. Hot secrets need the keychain unlocked: show
`pages/local/keys/unlock.rs::KeychainUnlock` inside a `modal` and retry on `UnlockEvent::Unlocked`.
`pages/local/keys/add_flow.rs::AddKeyFlow` is the embeddable add/import flow
(emits `AddKeyEvent::Added(id)` / `Cancelled`).

Screenshots: `/private/tmp/claude-501/tools/shot.sh out.png 7 /local/keys` (3rd arg =
`SIGVAULT_DEV_PAGE` path; data dir defaults to a seeded scratch dir with wallets
"Savings"/"Daily spend", passphrase `hunter22`). Copy the dir first if your flow writes to it:
`cp -R /private/tmp/claude-501/scratch/svdata-local /private/tmp/claude-501/scratch/<you>` and
pass `SIGVAULT_DATA_DIR=...`. Always look at your pages before reporting done.
