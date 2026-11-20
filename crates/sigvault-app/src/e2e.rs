//! UI driver for end-to-end tests (debug builds only).
//!
//! GPUI draws straight to the GPU, so there is no DOM or accessibility tree
//! for WebDriver to read. Instead, with `SIGVAULT_E2E_DRIVER=127.0.0.1:<port>`
//! set, a debug build listens on that address for newline-delimited JSON
//! commands, and the UI kit reports what it draws:
//!
//! - every widget that matters to a test reports itself with [`E2e::e2e`]
//!   (the kit's `button`, `checkbox`, `tabs`, `alert`, `TextInput`, page
//!   titles, …) or, when it is clickable, [`E2eClick::clickable`]. That records
//!   its role, label and on-screen bounds while the frame is laid out, plus
//!   the click handler;
//! - `snapshot` returns the current page path and those nodes;
//! - `click` runs a node's click handler with a synthetic left click at its
//!   center. It refuses nodes that are disabled, scrolled out of view or
//!   covered by a modal or menu (an `overlay` node drawn later), the cases
//!   where a user's click wouldn't land either. GPUI keeps
//!   `Window::dispatch_event` crate-private in 0.2, so this is as close to
//!   real mouse input as an app can get;
//! - `type` and `key` send keystrokes through `Window::dispatch_keystroke`,
//!   the path real typing takes, to whatever has focus.
//!
//! Commands (one JSON object per line, one reply per line):
//!
//! ```text
//! {"cmd":"snapshot"}
//! {"cmd":"click","label":"Continue","role":"button","exact":true,"index":0}
//! {"cmd":"type","text":"hello"}
//! {"cmd":"key","keys":"enter"}            // Keystroke::parse syntax, e.g. "cmd-a"
//! {"cmd":"quit"}
//! ```
//!
//! Replies are `{"ok":true,...}` or `{"ok":false,"error":"..."}`.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::OnceLock;

use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use gpui::{
    canvas, point, px, AnyElement, AnyWindowHandle, App, Bounds, ClickEvent, Keystroke, Modifiers,
    MouseButton, MouseClickEvent, MouseDownEvent, MouseUpEvent, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, Window,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const ENV: &str = "SIGVAULT_E2E_DRIVER";

/// The driver address, if this is a debug build and the env var is set.
fn address() -> Option<&'static str> {
    static ADDR: OnceLock<Option<String>> = OnceLock::new();
    ADDR.get_or_init(|| {
        if cfg!(debug_assertions) {
            std::env::var(ENV).ok().filter(|s| !s.is_empty())
        } else {
            None
        }
    })
    .as_deref()
}

pub fn enabled() -> bool {
    address().is_some()
}

pub type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One widget as drawn in the last frame.
#[derive(Clone, Serialize)]
pub struct Node {
    pub role: &'static str,
    pub label: String,
    pub enabled: bool,
    /// Center inside its clipping region and the window (not scrolled away).
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    pub bounds: [f32; 4],
    #[serde(skip)]
    rect: Bounds<Pixels>,
    #[serde(skip)]
    clip: Bounds<Pixels>,
    #[serde(skip)]
    on_click: Option<ClickHandler>,
}

thread_local! {
    static FRAME: RefCell<Vec<Node>> = const { RefCell::new(Vec::new()) };
}

/// Start a new frame: called by the root view before the tree is laid out.
pub fn begin_frame() {
    if enabled() {
        FRAME.with(|f| f.borrow_mut().clear());
    }
}

/// What a widget reports about itself.
#[derive(Clone, Default)]
pub struct Probe {
    on_click: Option<ClickHandler>,
    pub role: &'static str,
    pub label: String,
    pub disabled: bool,
    pub value: Option<String>,
    pub selected: Option<bool>,
    pub focused: Option<bool>,
}

impl Probe {
    pub fn new(role: &'static str, label: impl Into<String>) -> Self {
        Self {
            role,
            label: label.into(),
            ..Default::default()
        }
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn value(mut self, v: impl Into<String>) -> Self {
        self.value = Some(v.into());
        self
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = Some(s);
        self
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = Some(f);
        self
    }
    /// What `click` runs (usually the element's own `on_click`).
    pub fn on_click(mut self, f: ClickHandler) -> Self {
        self.on_click = Some(f);
        self
    }

    /// An invisible child covering its parent that records the parent's
    /// bounds during prepaint. `None` when the driver is off, so normal runs
    /// draw exactly the same tree as before.
    pub fn element(self) -> Option<AnyElement> {
        if !enabled() {
            return None;
        }
        use gpui::IntoElement;
        Some(
            canvas(
                move |rect, window, _cx| {
                    let clip = window.content_mask().bounds;
                    let viewport = Bounds::new(point(px(0.), px(0.)), window.viewport_size());
                    let c = rect.center();
                    let visible = clip.contains(&c) && viewport.contains(&c);
                    FRAME.with(|f| {
                        f.borrow_mut().push(Node {
                            role: self.role,
                            label: self.label,
                            enabled: !self.disabled,
                            visible,
                            value: self.value,
                            selected: self.selected,
                            focused: self.focused,
                            bounds: [
                                f32::from(rect.origin.x),
                                f32::from(rect.origin.y),
                                f32::from(rect.size.width),
                                f32::from(rect.size.height),
                            ],
                            rect,
                            clip,
                            on_click: self.on_click,
                        })
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        )
    }
}

/// `.e2e(Probe::new("button", "Join"))` on any element with children.
pub trait E2e: ParentElement + Sized {
    fn e2e(self, probe: Probe) -> Self {
        self.children(probe.element())
    }
}

impl<T: ParentElement> E2e for T {}

/// `.clickable(Probe::new("nav", "Keys"), handler)`: `on_click` plus a probe
/// the driver can click. Unless the probe is disabled, the element also
/// becomes a keyboard tab stop with the focus ring (Enter / Space click it),
/// see [`crate::ui::focus`].
pub trait E2eClick: StatefulInteractiveElement + ParentElement + Styled + Sized {
    fn clickable(
        mut self,
        probe: Probe,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if probe.disabled {
            return self.clickable_no_focus(probe, f);
        }
        let h: ClickHandler = Rc::new(f);
        let key = self
            .interactivity()
            .element_id
            .clone()
            .unwrap_or_else(|| gpui::ElementId::Name(probe.label.clone().into()));
        let radius = crate::ui::focus::radius_of(self.style())
            .unwrap_or(crate::ui::focus::DEFAULT_RING_RADIUS);
        // The focus target is absolutely positioned over this element.
        if self.style().position.is_none() {
            self = self.relative();
        }
        let h2 = h.clone();
        self.child(crate::ui::focus::focus_target(key, radius, h.clone()))
            .clickable_no_focus(probe, move |e, w, cx| h2(e, w, cx))
    }

    /// [`E2eClick::clickable`] without making the element a tab stop (for
    /// an element inside another tab stop, or a backdrop).
    fn clickable_no_focus(
        self,
        probe: Probe,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if !enabled() {
            return self.on_click(f);
        }
        let h: ClickHandler = Rc::new(f);
        let h2 = h.clone();
        self.on_click(move |e, w, cx| h2(e, w, cx))
            .children(probe.on_click(h).element())
    }
}

impl<T: StatefulInteractiveElement + ParentElement + Styled> E2eClick for T {}

// ─── Server ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    Snapshot,
    Click {
        label: String,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        exact: bool,
        #[serde(default)]
        index: usize,
    },
    Type {
        text: String,
    },
    Key {
        keys: String,
    },
    Quit,
}

type Job = (String, oneshot::Sender<String>);

/// Listen on `SIGVAULT_E2E_DRIVER` and serve requests against `window`.
pub fn start(window: AnyWindowHandle, cx: &mut App) {
    let Some(addr) = address() else {
        return;
    };
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            log::error!("e2e driver: cannot listen on {addr}: {e}");
            return;
        }
    };
    log::warn!("e2e driver listening on {addr}");
    let (tx, mut rx) = mpsc::unbounded::<Job>();

    // Socket I/O on plain threads; the UI work happens on the main thread.
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut writer = match stream.try_clone() {
                    Ok(w) => w,
                    Err(_) => return,
                };
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let (reply_tx, reply_rx) = oneshot::channel();
                    if tx.unbounded_send((line, reply_tx)).is_err() {
                        return;
                    }
                    let reply = futures::executor::block_on(reply_rx)
                        .unwrap_or_else(|_| error("app is shutting down"));
                    if writeln!(writer, "{reply}").is_err() {
                        break;
                    }
                }
            });
        }
    });

    cx.spawn(async move |cx| {
        while let Some((line, reply)) = rx.next().await {
            let out = match serde_json::from_str::<Request>(&line) {
                Err(e) => error(&format!("bad request: {e}")),
                Ok(Request::Quit) => {
                    reply.send(ok(json!({}))).ok();
                    cx.update(|cx| cx.quit()).ok();
                    return;
                }
                Ok(req) => cx
                    .update(|cx| {
                        window
                            .update(cx, |_, window, cx| handle(req, window, cx))
                            .unwrap_or_else(|e| error(&format!("window gone: {e}")))
                    })
                    .unwrap_or_else(|e| error(&format!("app gone: {e}"))),
            };
            reply.send(out).ok();
        }
    })
    .detach();
}

fn ok(mut v: Value) -> String {
    v["ok"] = Value::Bool(true);
    v.to_string()
}

fn error(msg: &str) -> String {
    json!({ "ok": false, "error": msg }).to_string()
}

fn handle(req: Request, window: &mut Window, cx: &mut App) -> String {
    match req {
        Request::Snapshot => {
            let nodes = FRAME.with(|f| f.borrow().clone());
            let size = window.viewport_size();
            ok(json!({
                "page": crate::shell::current_path(cx),
                "window": [f32::from(size.width), f32::from(size.height)],
                "nodes": nodes,
            }))
        }
        Request::Click {
            label,
            role,
            exact,
            index,
        } => click(window, cx, &label, role.as_deref(), exact, index),
        Request::Type { text } => {
            for ch in text.chars() {
                let s = ch.to_string();
                let key = match ch {
                    ' ' => "space".to_string(),
                    c => c.to_lowercase().to_string(),
                };
                let ks = Keystroke {
                    modifiers: Modifiers {
                        shift: ch.is_uppercase(),
                        ..Default::default()
                    },
                    key,
                    key_char: Some(s),
                };
                if !window.dispatch_keystroke(ks, cx) {
                    return error("no focused text input took the keystroke");
                }
            }
            ok(json!({}))
        }
        Request::Key { keys } => {
            for k in keys.split_whitespace() {
                match Keystroke::parse(k) {
                    Ok(ks) => {
                        window.dispatch_keystroke(ks, cx);
                    }
                    Err(e) => return error(&format!("bad keystroke {k:?}: {e}")),
                }
            }
            ok(json!({}))
        }
        Request::Quit => unreachable!("handled by the server loop"),
    }
}

fn click(
    window: &mut Window,
    cx: &mut App,
    label: &str,
    role: Option<&str>,
    exact: bool,
    index: usize,
) -> String {
    let want = label.to_lowercase();
    let frame = FRAME.with(|f| f.borrow().clone());
    let matches: Vec<(usize, &Node)> = frame
        .iter()
        .enumerate()
        .filter(|(_, n)| role.is_none_or(|r| r == n.role))
        .filter(|(_, n)| {
            if exact {
                n.label == label
            } else {
                n.label.to_lowercase().contains(&want)
            }
        })
        .collect();
    let Some(&(pos, node)) = matches.get(index) else {
        return error(&format!(
            "no {} matching {label:?} (found {})",
            role.unwrap_or("node"),
            matches.len()
        ));
    };
    let what = format!("{} {label:?}", node.role);
    if !node.enabled {
        return error(&format!("{what} is disabled"));
    }
    // A node scrolled out of its container still clicks: the driver has no
    // way to scroll, and a user would simply scroll to it.
    let target = node.rect.center();
    // Modals and menus register an `overlay` node before their content, so
    // an overlay drawn after this node and covering it is on top of it.
    if let Some(cover) = frame[pos + 1..]
        .iter()
        .find(|n| n.role == "overlay" && n.rect.contains(&target))
    {
        return error(&format!("{what} is covered by {:?}", cover.label));
    }
    let Some(handler) = node.on_click.clone() else {
        return error(&format!("{what} is not clickable"));
    };
    let down = MouseDownEvent {
        button: MouseButton::Left,
        position: target,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    };
    let up = MouseUpEvent {
        button: MouseButton::Left,
        position: target,
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    handler(&ClickEvent::Mouse(MouseClickEvent { down, up }), window, cx);
    window.refresh();
    ok(json!({ "clicked": node }))
}
