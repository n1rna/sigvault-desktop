use gpui::{
    anchored, deferred, div, point, prelude::*, px, AnyElement, App, Bounds, ClickEvent, ElementId,
    FocusHandle, KeyboardButton, KeyboardClickEvent, SharedString, Window,
};

use super::focus::{cycle, Cancel, FocusNext, FocusPrev};
use crate::e2e::{E2e, Probe};
use crate::theme::{theme, RADIUS_CARD};

type DismissHandler = std::rc::Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Centered dialog over a dimmed backdrop that covers the whole window
/// (sidebar and top bar included, so nothing outside the dialog takes
/// clicks). Render it as the last child of a page.
///
/// Keyboard: focus moves into the dialog when it opens (unless the page
/// already focused something inside it, e.g. a passphrase field), Tab and
/// Shift-Tab stay inside it, and Esc calls `on_dismiss` like a backdrop
/// click does (with a `ClickEvent::Keyboard`).
///
/// ```ignore
/// .when(self.show_dialog, |d| d.child(
///     modal("unlock", 420.).on_dismiss(cx.listener(|this, _, _, cx| { this.show_dialog = false; cx.notify(); }))
///         .child(heading("Unlock wallet")) ...
/// ))
/// ```
pub fn modal(id: impl Into<ElementId>, width: f32) -> Modal {
    Modal {
        id: id.into(),
        width,
        on_dismiss: None,
        title: None,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    width: f32,
    on_dismiss: Option<DismissHandler>,
    title: Option<(SharedString, Option<SharedString>)>,
    children: Vec<AnyElement>,
}

impl Modal {
    /// Called when the backdrop is clicked or Esc is pressed.
    pub fn on_dismiss(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(std::rc::Rc::new(f));
        self
    }

    /// A heading (and optional description) above the content, with a
    /// close button when the dialog has `on_dismiss`.
    pub fn title(
        mut self,
        title: impl Into<SharedString>,
        description: Option<SharedString>,
    ) -> Self {
        self.title = Some((title.into(), description));
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Per-open focus state of a dialog.
struct ModalFocus {
    handle: FocusHandle,
    claimed: bool,
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme();
        let state = window.use_keyed_state(
            ElementId::NamedChild(Box::new(self.id.clone()), SharedString::from("focus")),
            cx,
            |_, cx| ModalFocus {
                handle: cx.focus_handle(),
                claimed: false,
            },
        );
        let handle = state.read(cx).handle.clone();
        if !state.read(cx).claimed {
            state.update(cx, |s, _| s.claimed = true);
            let h = handle.clone();
            window.defer(cx, move |window, cx| {
                if !h.contains_focused(window, cx) {
                    window.focus(&h);
                }
            });
        }

        let viewport = window.viewport_size();
        let mut backdrop = div()
            .id(self.id)
            .e2e(Probe::new("overlay", "modal"))
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .bg(t.scrim)
            .flex()
            .items_center()
            .justify_center()
            .p_6();
        if let Some(f) = self.on_dismiss.clone() {
            backdrop = backdrop.on_click(move |e, w, cx| f(e, w, cx));
        }
        let (h_next, h_prev) = (handle.clone(), handle.clone());
        let dismiss = self.on_dismiss;
        let header = self.title.map(|(title, description)| {
            let close = dismiss.clone();
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.foreground)
                                .child(title),
                        )
                        .children(description.map(|d| {
                            div()
                                .text_size(px(13.))
                                .line_height(px(19.))
                                .text_color(t.muted_foreground)
                                .child(d)
                        })),
                )
                .children(close.map(|f| {
                    super::icon_button("modal-close", "x")
                        .e2e_label("Close")
                        .on_click(move |e, w, cx| f(e, w, cx))
                }))
        });
        // Anchored at the window origin so the scrim covers the whole
        // window, not just the page that renders the dialog.
        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                backdrop.child(
                    div()
                        .id("modal-panel")
                        .track_focus(&handle)
                        .on_action(move |_: &FocusNext, window, cx| {
                            cycle(true, Some(&h_next), window, cx)
                        })
                        .on_action(move |_: &FocusPrev, window, cx| {
                            cycle(false, Some(&h_prev), window, cx)
                        })
                        .on_action(move |_: &Cancel, window, cx| {
                            if let Some(f) = &dismiss {
                                let ev = ClickEvent::Keyboard(KeyboardClickEvent {
                                    button: KeyboardButton::Enter,
                                    bounds: Bounds::new(point(px(0.), px(0.)), Default::default()),
                                });
                                f(&ev, window, cx);
                                window.refresh();
                            }
                        })
                        .w(px(self.width))
                        .max_w_full()
                        .max_h_full()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_6()
                        .rounded(RADIUS_CARD)
                        .border_1()
                        .border_color(t.line_tertiary)
                        .bg(t.page)
                        .shadow_lg()
                        // Swallow clicks so they don't reach the backdrop.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .children(header)
                        .children(self.children),
                ),
            ),
        )
        .with_priority(10)
    }
}
