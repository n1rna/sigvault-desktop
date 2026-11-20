//! Page frame: the one layout every in-app page uses, so headers, padding
//! and content width are identical across the app.
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │ Wallets › Savings                  (crumbs)    │  header (fixed)
//! │ Title                        [act] [act] [CTA] │
//! │ Subtitle                                       │
//! │ [header_extra: stepper / tabs]                 │
//! ├───────────────────────────────────────────────┤
//! │ content … (scrolls)                            │  body
//! ├───────────────────────────────────────────────┤
//! │ [footer: Back            Cancel  Continue →]   │  footer (fixed, optional)
//! └───────────────────────────────────────────────┘
//! ```
//!
//! ```ignore
//! page_frame("wallets")
//!     .title("Wallets")
//!     .subtitle("On-device wallets…")
//!     .action(button("new", "New wallet").icon("plus").on_click(..))
//!     .child(list)
//! ```

use gpui::{div, prelude::*, px, AnyElement, App, ElementId, Pixels, SharedString, Window};

use super::common::{icon, section_head};
use crate::e2e::{E2e, E2eClick, Probe};
use crate::router::Page;
use crate::shell::navigate;
use crate::theme::{alpha, theme};

/// Max width of page content. Header, body and footer share it so their
/// left/right edges line up on every page.
pub const CONTENT_MAX_WIDTH: Pixels = px(1120.);
/// Horizontal page padding.
pub const PAGE_PADDING_X: Pixels = px(40.);
/// Narrower column for single-form steps (wizard steps, settings forms).
/// Use inside the body: `div().max_w(FORM_WIDTH)`.
pub const FORM_WIDTH: Pixels = px(640.);

pub fn page_frame(id: impl Into<ElementId>) -> PageFrame {
    PageFrame {
        id: id.into(),
        crumbs: Vec::new(),
        title: None,
        title_adornment: None,
        subtitle: None,
        actions: Vec::new(),
        header_extra: None,
        footer: None,
        children: Vec::new(),
        scroll: true,
    }
}

#[derive(IntoElement)]
pub struct PageFrame {
    id: ElementId,
    crumbs: Vec<(SharedString, Option<Page>)>,
    title: Option<SharedString>,
    title_adornment: Option<AnyElement>,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
    header_extra: Option<AnyElement>,
    footer: Option<AnyElement>,
    children: Vec<AnyElement>,
    scroll: bool,
}

impl PageFrame {
    /// Breadcrumb entry. `Some(page)` makes it a link.
    pub fn crumb(mut self, label: impl Into<SharedString>, page: Option<Page>) -> Self {
        self.crumbs.push((label.into(), page));
        self
    }
    pub fn title(mut self, t: impl Into<SharedString>) -> Self {
        self.title = Some(t.into());
        self
    }
    /// Element shown right after the title (badges, status dot).
    pub fn title_adornment(mut self, el: impl IntoElement) -> Self {
        self.title_adornment = Some(el.into_any_element());
        self
    }
    pub fn subtitle(mut self, s: impl Into<SharedString>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// Header action (right-aligned, in order). Put the primary CTA last.
    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.actions.push(el.into_any_element());
        self
    }
    /// Extra header row below the title (stepper, tabs, filters).
    pub fn header_extra(mut self, el: impl IntoElement) -> Self {
        self.header_extra = Some(el.into_any_element());
        self
    }
    /// Fixed footer bar — wizards put Back / Continue here.
    pub fn footer(mut self, el: impl IntoElement) -> Self {
        self.footer = Some(el.into_any_element());
        self
    }
    /// Disable body scrolling (the page manages its own scroll areas).
    pub fn no_scroll(mut self) -> Self {
        self.scroll = false;
        self
    }
}

impl ParentElement for PageFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Footer layout helper: `left` on the left, `right` grouped on the right.
pub fn footer_bar(left: Vec<AnyElement>, right: Vec<AnyElement>) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .children(left),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .children(right),
        )
}

/// A titled section inside a page body (`Recent activity`, `Keys`, …).
pub fn section(title: impl Into<SharedString>) -> Section {
    Section {
        title: title.into(),
        description: None,
        action: None,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct Section {
    title: SharedString,
    description: Option<SharedString>,
    action: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn description(mut self, d: impl Into<SharedString>) -> Self {
        self.description = Some(d.into());
        self
    }
    pub fn action(mut self, el: impl IntoElement) -> Self {
        self.action = Some(el.into_any_element());
        self
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Section {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let t = theme();
        // Web `SectionHead`: mono uppercase label + hairline rule + trailing action.
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(section_head(self.title, self.action))
                    .children(self.description.map(|d| {
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(t.ink_2)
                            .child(d)
                    })),
            )
            .children(self.children)
    }
}

impl RenderOnce for PageFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let t = theme();
        let n_crumbs = self.crumbs.len();

        // Web NavigationBreadcrumbs: muted links, current page in ink.
        let crumbs = (n_crumbs > 0).then(|| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .mb_4()
                .text_size(px(14.))
                .text_color(t.ink_2)
                .children(
                    self.crumbs
                        .into_iter()
                        .enumerate()
                        .map(move |(i, (label, page))| {
                            let last = i + 1 == n_crumbs;
                            let label_text = label.to_string();
                            let mut el = div()
                                .id(SharedString::from(format!("crumb-{i}")))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .when(i > 0, |d| {
                                    d.child(icon("chevron-right").size(px(13.)).text_color(t.ink_3))
                                })
                                .child(
                                    div()
                                        .when(last, |d| {
                                            d.text_color(t.ink)
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                        })
                                        .child(label),
                                );
                            if let Some(page) = page.filter(|_| !last) {
                                el = el
                                    .cursor_pointer()
                                    .hover(move |s| s.text_color(t.ink))
                                    .clickable(
                                        Probe::new("link", label_text.clone()),
                                        move |_, _, cx| navigate(page.clone(), cx),
                                    );
                            }
                            el
                        }),
                )
        });

        // Web PageHeader: big title, description, actions bottom-aligned, rule.
        let header = div()
            .flex()
            .flex_col()
            .gap_6()
            .border_b_1()
            .border_color(t.line)
            .pb_8()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .justify_between()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .children(self.title.map(|title| {
                                        div()
                                            .e2e(Probe::new("heading", title.to_string()))
                                            .truncate()
                                            .text_size(px(34.))
                                            .line_height(px(40.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(t.ink)
                                            .child(title)
                                    }))
                                    .children(self.title_adornment),
                            )
                            .children(self.subtitle.map(|s| {
                                div()
                                    .text_size(px(14.))
                                    .line_height(px(20.))
                                    .text_color(t.ink_2)
                                    .child(s)
                            })),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .children(self.actions),
                    ),
            )
            .children(self.header_extra);

        let column = div()
            .w_full()
            .max_w(CONTENT_MAX_WIDTH)
            .flex()
            .flex_col()
            .children(crumbs)
            .child(header)
            .child(
                div()
                    .pt_8()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .children(self.children),
            );

        let body_inner = div()
            .w_full()
            .flex()
            .justify_center()
            .px(PAGE_PADDING_X)
            .pt_8()
            .pb_10()
            .child(column);
        let body = if self.scroll {
            div()
                .id(self.id)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body_inner)
                .into_any_element()
        } else {
            div()
                .id(self.id)
                .flex_1()
                .min_h_0()
                .child(body_inner)
                .into_any_element()
        };

        let footer = self.footer.map(|f| {
            div()
                .flex_none()
                .w_full()
                .flex()
                .justify_center()
                .px(PAGE_PADDING_X)
                .py(px(12.))
                .border_t_1()
                .border_color(t.line)
                .bg(t.canvas)
                .child(div().w_full().max_w(CONTENT_MAX_WIDTH).child(f))
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(body)
            .children(footer)
    }
}
