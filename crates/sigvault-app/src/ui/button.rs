use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, SharedString, Window,
};

use super::common::{icon, spinner_with};
use crate::theme::{alpha, theme, RADIUS_CHIP, RADIUS_CONTROL};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
    Icon,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// shadcn-style button. `button("id", "Label").on_click(...)`.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    icon_right: Option<&'static str>,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    loading: bool,
    full_width: bool,
    on_click: Option<ClickHandler>,
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button {
        id: id.into(),
        label: Some(label.into()),
        icon: None,
        icon_right: None,
        variant: ButtonVariant::Primary,
        size: ButtonSize::Md,
        disabled: false,
        loading: false,
        full_width: false,
        on_click: None,
    }
}

pub fn icon_button(id: impl Into<ElementId>, icon_name: &'static str) -> Button {
    Button {
        id: id.into(),
        label: None,
        icon: Some(icon_name),
        icon_right: None,
        variant: ButtonVariant::Ghost,
        size: ButtonSize::Icon,
        disabled: false,
        loading: false,
        full_width: false,
        on_click: None,
    }
}

impl Button {
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn secondary(self) -> Self {
        self.variant(ButtonVariant::Secondary)
    }
    pub fn outline(self) -> Self {
        self.variant(ButtonVariant::Outline)
    }
    pub fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }
    pub fn destructive(self) -> Self {
        self.variant(ButtonVariant::Destructive)
    }
    pub fn link(self) -> Self {
        self.variant(ButtonVariant::Link)
    }
    pub fn size(mut self, s: ButtonSize) -> Self {
        self.size = s;
        self
    }
    pub fn small(self) -> Self {
        self.size(ButtonSize::Sm)
    }
    pub fn large(self) -> Self {
        self.size(ButtonSize::Lg)
    }
    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }
    pub fn icon_right(mut self, name: &'static str) -> Self {
        self.icon_right = Some(name);
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// Shows a spinner in place of the icon and disables clicks.
    pub fn loading(mut self, l: bool) -> Self {
        self.loading = l;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let t = theme();
        let inert = self.disabled || self.loading;
        // Web `button.tsx`: rounded-control, shadow-btn ring (= 1px line-strong).
        let (bg, fg, hover_bg, border) = match self.variant {
            ButtonVariant::Primary => (Some(t.brand), t.brand_contrast, Some(alpha(t.brand, 0.9)), None),
            ButtonVariant::Secondary => (Some(t.inset), t.ink, Some(t.hover_strong), Some(t.line_strong)),
            ButtonVariant::Outline => (Some(t.surface), t.ink, Some(t.inset), Some(t.line_strong)),
            ButtonVariant::Ghost => (None, t.ink_2, Some(t.hover_strong), None),
            ButtonVariant::Destructive => (Some(t.destructive), t.destructive_foreground, Some(alpha(t.destructive, 0.9)), None),
            ButtonVariant::Link => (None, t.brand_ink, None, None),
        };
        let (h, px_, text, icon_size, radius) = match self.size {
            ButtonSize::Sm => (px(32.), px(12.), px(12.), px(14.), RADIUS_CHIP),
            ButtonSize::Md => (px(36.), px(16.), px(14.), px(16.), RADIUS_CONTROL),
            ButtonSize::Lg => (px(40.), px(28.), px(14.), px(16.), RADIUS_CONTROL),
            ButtonSize::Icon => (px(36.), px(0.), px(14.), px(16.), RADIUS_CONTROL),
        };

        let mut el = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_2()
            .h(h)
            .px(px_)
            .rounded(radius)
            .text_size(text)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(fg)
            .whitespace_nowrap();
        if self.size == ButtonSize::Icon {
            el = el.w(h);
        }
        if self.full_width {
            el = el.w_full();
        }
        if let Some(bg) = bg {
            el = el.bg(bg);
        }
        if let Some(b) = border {
            el = el.border_1().border_color(b);
        }
        if inert {
            el = el.opacity(0.5);
        } else {
            el = el.cursor_pointer();
            if let Some(hbg) = hover_bg {
                let is_ghost = self.variant == ButtonVariant::Ghost;
                el = el.hover(move |s| {
                    let s = s.bg(hbg);
                    if is_ghost { s.text_color(t.ink) } else { s }
                });
            } else {
                el = el.hover(|s| s.underline());
            }
            if let Some(handler) = self.on_click {
                el = el.on_click(handler);
            }
        }

        let leading: Option<AnyElement> = if self.loading {
            Some(spinner_with(icon_size, fg).into_any_element())
        } else {
            self.icon.map(|n| icon(n).size(icon_size).text_color(fg).into_any_element())
        };

        el.children(leading)
            .children(self.label)
            .children(self.icon_right.map(|n| icon(n).size(icon_size).text_color(fg)))
    }
}
