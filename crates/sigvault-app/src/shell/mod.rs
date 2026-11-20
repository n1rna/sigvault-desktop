//! Application chrome: root view, page host, top bar, activity panel,
//! update banner and toast. Equivalent of `GlobalLayout` + `AppRouter` +
//! `ActivityPanel` in `App.tsx`.

mod activity;
pub mod sidebar;
mod update;

use std::time::Duration;

use gpui::{
    div, prelude::*, px, AnyView, App, Context, Entity, FocusHandle, Global, PromptLevel,
    SharedString, Subscription, WeakEntity, Window,
};
use sigvault_core::app_mode::AppMode;

use crate::e2e::{E2e, Probe};
use crate::pages;
use crate::router::{resolve, Page};
use crate::state::{store, StoreEvent};
use crate::theme::{theme, FONT_SANS, RADIUS_CARD};
use crate::ui::icon;

pub use activity::ActivityPanel;
pub use sidebar::{switch_space, AppSidebar};
pub use update::UpdateBanner;

pub struct Root {
    current: Page,
    pending: Option<Page>,
    view: Option<AnyView>,
    sidebar: Entity<AppSidebar>,
    dev_page: Option<Page>,
    activity: Entity<ActivityPanel>,
    update: Entity<UpdateBanner>,
    toast: Option<SharedString>,
    toast_epoch: usize,
    /// Fallback focus: keeps the root on the key-dispatch path so Tab, Esc
    /// and the window actions work when nothing else has focus.
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

struct RootHandle(WeakEntity<Root>);
impl Global for RootHandle {}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = crate::theme::observe_window(window, cx);
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let st = store(cx);
        let sub = cx.subscribe(&st, |this, _, ev: &StoreEvent, cx| {
            if matches!(ev, StoreEvent::RouteChanged) {
                // Re-assert the backend route against the current page.
                let current = this.pending.clone().unwrap_or_else(|| this.current.clone());
                this.go(current, cx);
                // Debug builds: jump to SIGVAULT_DEV_PAGE once the backend
                // has routed (handy for screenshots of deep pages).
                if let Some(page) = this.dev_page.take() {
                    this.go(page, cx);
                }
            }
        });
        cx.set_global(RootHandle(cx.entity().downgrade()));
        Self {
            current: Page::Loading,
            pending: Some(Page::Loading),
            view: None,
            sidebar: {
                let sb = cx.new(AppSidebar::new);
                cx.observe(&sb, |_, _, cx| cx.notify()).detach();
                sb
            },
            dev_page: if cfg!(debug_assertions) {
                std::env::var("SIGVAULT_DEV_PAGE")
                    .ok()
                    .and_then(|p| Page::from_path(&p))
            } else {
                None
            },
            activity: cx.new(ActivityPanel::new),
            update: cx.new(UpdateBanner::new),
            toast: None,
            toast_epoch: 0,
            focus_handle,
            _subs: vec![sub, appearance],
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
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
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

/// ⌘, : the Settings page of the workspace on screen.
pub fn open_settings(cx: &mut App) {
    let Some(root) = cx.try_global::<RootHandle>().and_then(|h| h.0.upgrade()) else {
        return;
    };
    let current = root.read(cx).current.clone();
    let page = if current.is_cloud() {
        Page::CloudSettings
    } else if current.is_local() {
        Page::LocalSettings
    } else {
        match store(cx).read(cx).app_mode {
            Some(AppMode::Cloud) => Page::CloudSettings,
            Some(AppMode::Local) => Page::LocalSettings,
            None => return,
        }
    };
    navigate(page, cx);
}

/// Path of the page on screen (for the e2e driver).
pub fn current_path(cx: &App) -> String {
    cx.try_global::<RootHandle>()
        .and_then(|h| h.0.upgrade())
        .map(|root| root.read(cx).current.path())
        .unwrap_or_default()
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
        crate::e2e::begin_frame();
        if let Some(page) = self.pending.take() {
            self.view = Some(pages::build(&page, window, cx));
            self.sidebar
                .update(cx, |s, cx| s.set_current(page.clone(), cx));
            self.current = page;
        }
        let with_chrome = self.current.has_sidebar();
        let t = theme();
        // The focused element went away (page change, closed menu): fall
        // back to the root so keyboard shortcuts keep working.
        if window.focused(cx).is_none() {
            let h = self.focus_handle.clone();
            window.defer(cx, move |window, cx| {
                if window.focused(cx).is_none() {
                    window.focus(&h);
                }
            });
        }

        // Web shell: canvas floor; full-width top bar; sidebar; content as an
        // inset `surface` panel with a rounded top-left corner and hairline.
        let topbar = if with_chrome {
            self.sidebar.update(cx, |s, cx| s.render_topbar(cx))
        } else {
            sidebar::bare_titlebar().into_any_element()
        };
        let content = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(t.surface)
            .when(with_chrome, |d| {
                d.rounded_tl(RADIUS_CARD)
                    .border_l_1()
                    .border_t_1()
                    .border_color(t.line)
            })
            .children(self.view.clone());

        div()
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .on_action(|_: &crate::ui::focus::FocusNext, window, cx| {
                crate::ui::focus::cycle(true, None, window, cx)
            })
            .on_action(|_: &crate::ui::focus::FocusPrev, window, cx| {
                crate::ui::focus::cycle(false, None, window, cx)
            })
            .on_action(|_: &crate::Minimize, window, _| window.minimize_window())
            .on_action(|_: &crate::Zoom, window, _| window.zoom_window())
            .on_action(|_: &crate::CloseWindow, window, _| window.remove_window())
            .on_action(|_: &crate::About, window, cx| {
                let detail = format!(
                    "Version {}\n\nOpen source; builds reproducibly from n1rna/sigvault-desktop.",
                    env!("CARGO_PKG_VERSION")
                );
                // The only answer is OK; nothing to wait for.
                drop(window.prompt(
                    PromptLevel::Info,
                    "SigVault Desktop",
                    Some(&detail),
                    &["OK"],
                    cx,
                ));
            })
            .flex()
            .flex_col()
            .bg(if with_chrome { t.canvas } else { t.surface })
            .font_family(FONT_SANS)
            .text_color(t.ink)
            .text_size(px(14.))
            .child(topbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .when(with_chrome, |d| d.child(self.sidebar.clone()))
                    .child(content),
            )
            .child(self.activity.clone())
            .child(self.update.clone())
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
                            .border_color(t.line_tertiary)
                            .bg(t.surface_3)
                            .shadow_lg()
                            .text_size(px(13.))
                            .child(icon("check").size(px(14.)).text_color(t.success))
                            .e2e(Probe::new("toast", msg.to_string()))
                            .child(msg),
                    )
            }))
    }
}
