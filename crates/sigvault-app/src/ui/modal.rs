use gpui::{deferred, div, hsla, prelude::*, px, AnyElement, App, ClickEvent, ElementId, Window};

use crate::e2e::{E2e, Probe};
use crate::theme::{theme, RADIUS_CARD};

type DismissHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Centered dialog over a dimmed backdrop. Render it as the last child of a
/// page (pages are `relative` + full size, so the overlay covers the page).
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
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    width: f32,
    on_dismiss: Option<DismissHandler>,
    children: Vec<AnyElement>,
}

impl Modal {
    /// Called when the backdrop is clicked.
    pub fn on_dismiss(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(f));
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Modal {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let t = theme();
        let mut backdrop = div()
            .id(self.id)
            .e2e(Probe::new("overlay", "modal"))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(hsla(0., 0., 0., 0.6))
            .flex()
            .items_center()
            .justify_center()
            .p_6();
        if let Some(f) = self.on_dismiss {
            backdrop = backdrop.on_click(f);
        }
        deferred(
            backdrop.child(
                div()
                    .id("modal-panel")
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
                    .children(self.children),
            ),
        )
        .with_priority(10)
    }
}
