//! Application chrome: root view, page host, top bar, activity panel,
//! update banner and toast. Equivalent of `GlobalLayout` + `AppRouter` +
//! `ActivityPanel` in `App.tsx`.

mod activity;
mod topbar;
mod update;

use std::time::Duration;

use gpui::{
    div, prelude::*, px, AnyView, App, Context, Entity, Global, SharedString, Subscription,
    WeakEntity, Window,
};

use crate::pages;
use crate::router::{resolve, Page};
use crate::state::{store, StoreEvent};
use crate::theme::{alpha, theme};
use crate::ui::icon;

pub use activity::ActivityPanel;
pub use topbar::TopBar;
pub use update::UpdateBanner;

pub struct Root {
    current: Page,
    pending: Option<Page>,
    view: Option<AnyView>,
    topbar: Entity<TopBar>,
    activity: Entity<ActivityPanel>,
    update: Entity<UpdateBanner>,
    toast: Option<SharedString>,
    toast_epoch: usize,
    _subs: Vec<Subscription>,
}

struct RootHandle(WeakEntity<Root>);
impl Global for RootHandle {}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let st = store(cx);
        let sub = cx.subscribe(&st, |this, _, ev: &StoreEvent, cx| {
            if matches!(ev, StoreEvent::RouteChanged) {
                // Re-assert the backend route against the current page.
                let current = this.pending.clone().unwrap_or_else(|| this.current.clone());
                this.go(current, cx);
            }
        });
        cx.set_global(RootHandle(cx.entity().downgrade()));
        Self {
            current: Page::Loading,
            pending: Some(Page::Loading),
            view: None,
            topbar: cx.new(|cx| TopBar::new(window, cx)),
            activity: cx.new(|cx| ActivityPanel::new(cx)),
            update: cx.new(|cx| UpdateBanner::new(cx)),
            toast: None,
            toast_epoch: 0,
            _subs: vec![sub],
        }
    }

    fn go(&mut self, requested: Page, cx: &mut Context<Self>) {
        let route = store(cx).read(cx).route.clone();
        let page = resolve(&route, &requested);
        if page == self.current && self.view.is_some() && self.pending.is_none() {
            return;
        }
        self.pending = Some(page);
        cx.notify();
    }

    fn show_toast(&mut self, msg: SharedString, cx: &mut Context<Self>) {
        self.toast = Some(msg);
        self.toast_epoch += 1;
        let epoch = self.toast_epoch;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1800)).await;
            this.update(cx, |this, cx| {
                if this.toast_epoch == epoch {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

/// Navigate to a page (subject to the backend route, see [`resolve`]).
/// Pages are rebuilt on navigation, like a React route remount.
pub fn navigate(page: Page, cx: &mut App) {
    let Some(root) = cx.try_global::<RootHandle>().and_then(|h| h.0.upgrade()) else {
        return;
    };
    root.update(cx, |root, cx| root.go(page, cx));
}

/// Show a short-lived toast at the bottom of the window.
pub fn toast(cx: &mut App, msg: impl Into<SharedString>) {
    let msg = msg.into();
    let Some(root) = cx.try_global::<RootHandle>().and_then(|h| h.0.upgrade()) else {
        return;
    };
    root.update(cx, |root, cx| root.show_toast(msg, cx));
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(page) = self.pending.take() {
            self.view = Some(pages::build(&page, window, cx));
            self.current = page;
        }
        let t = theme();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(t.background)
            .text_color(t.foreground)
            .text_size(px(14.))
            .child(self.topbar.clone())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .overflow_hidden()
                    .children(self.view.clone()),
            )
            .child(self.update.clone())
            .child(self.activity.clone())
            .children(self.toast.clone().map(|msg| {
                div()
                    .absolute()
                    .bottom(px(56.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_4()
                            .py_2()
                            .rounded_full()
                            .border_1()
                            .border_color(t.border)
                            .bg(alpha(t.card, 0.97))
                            .shadow_lg()
                            .text_size(px(12.))
                            .child(icon("check").size(px(13.)).text_color(t.success))
                            .child(msg),
                    )
            }))
    }
}
