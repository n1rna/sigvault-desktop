//! Keyboard focus: Tab / Shift-Tab order, the focus ring and Esc.
//!
//! - Every element made clickable through [`crate::e2e::E2eClick::clickable`]
//!   (and every enabled kit [`super::Button`]) is a tab stop: Tab reaches it,
//!   Enter or Space clicks it, and a 2px `ring` outline shows while it has
//!   focus. The ring follows the element's corner radius when `.rounded(..)`
//!   is set before `.clickable(..)`.
//! - It works by adding a [`FocusTarget`] child: an invisible box covering
//!   the element (3px outset) that owns the focus handle, draws the ring and
//!   turns Enter / Space into a `ClickEvent::Keyboard` for the element's
//!   handler. Mouse presses don't move focus to it (macOS behaviour), so
//!   the ring only appears for keyboard focus.
//! - For a hand-rolled clickable that can't use `clickable`, add
//!   `focus_target(key, radius, handler)` as a child of a `relative()` parent.
//! - [`Cancel`] (Esc) closes the topmost modal; modals also keep Tab inside
//!   themselves (see `ui/modal.rs`).

use gpui::{
    actions, div, point, prelude::*, px, AbsoluteLength, App, Bounds, ClickEvent, ElementId,
    KeyBinding, KeyboardButton, KeyboardClickEvent, MouseButton, Pixels, SharedString, Window,
};

use crate::e2e::ClickHandler;
use crate::theme::theme;

actions!(focus, [FocusNext, FocusPrev, Cancel]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrev, None),
        KeyBinding::new("escape", Cancel, None),
    ]);
}

/// Ring outset from the element's edge.
const RING_OUTSET: f32 = 3.;
/// Radius used when the element's radius isn't known.
pub const DEFAULT_RING_RADIUS: Pixels = px(8.);

/// The element's top-left radius in px, if it was set with `.rounded(..)`.
pub fn radius_of(style: &gpui::StyleRefinement) -> Option<Pixels> {
    match style.corner_radii.top_left {
        Some(AbsoluteLength::Pixels(p)) => Some(p),
        Some(AbsoluteLength::Rems(r)) => Some(px(r.0 * 16.)),
        None => None,
    }
}

/// An invisible tab stop covering its parent (see the module docs). The
/// parent must be positioned (`relative()`, which `clickable` ensures).
pub fn focus_target(
    key: impl Into<ElementId>,
    radius: Pixels,
    handler: ClickHandler,
) -> FocusTarget {
    FocusTarget {
        key: key.into(),
        radius,
        handler,
    }
}

#[derive(IntoElement)]
pub struct FocusTarget {
    key: ElementId,
    radius: Pixels,
    handler: ClickHandler,
}

impl RenderOnce for FocusTarget {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = ElementId::NamedChild(Box::new(self.key), SharedString::from("focus"));
        let handle = window
            .use_keyed_state(key.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
            .read(cx)
            .clone();
        let focused = handle.is_focused(window);
        let handler = self.handler;
        let o = px(-RING_OUTSET);
        // Cap huge radii (rounded_full) so the ring stays a pill.
        let radius = self.radius.min(px(999.)) + px(RING_OUTSET);
        div()
            .id(key)
            .absolute()
            .top(o)
            .left(o)
            .right(o)
            .bottom(o)
            .rounded(radius)
            .track_focus(&handle)
            // A press on the element doesn't take focus (no ring on click).
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_key_down(move |ev, window, cx| {
                let k = &ev.keystroke;
                let button = match k.key.as_str() {
                    "enter" => KeyboardButton::Enter,
                    "space" => KeyboardButton::Space,
                    _ => return,
                };
                if ev.is_held || k.modifiers.modified() {
                    return;
                }
                cx.stop_propagation();
                let click = ClickEvent::Keyboard(KeyboardClickEvent {
                    button,
                    bounds: Bounds::new(point(px(0.), px(0.)), Default::default()),
                });
                handler(&click, window, cx);
                window.refresh();
            })
            .when(focused, |d| d.border_2().border_color(theme().ring))
    }
}

/// Move focus to the next / previous tab stop, staying inside `scope` when
/// given (modals trap Tab).
pub fn cycle(forward: bool, scope: Option<&gpui::FocusHandle>, window: &mut Window, cx: &App) {
    let step = |window: &mut Window| {
        if forward {
            window.focus_next()
        } else {
            window.focus_prev()
        }
    };
    step(window);
    let Some(scope) = scope else {
        return;
    };
    // Tab stops are few; bail out rather than spin if the scope has none.
    for _ in 0..256 {
        if scope.contains_focused(window, cx) {
            return;
        }
        step(window);
    }
}
