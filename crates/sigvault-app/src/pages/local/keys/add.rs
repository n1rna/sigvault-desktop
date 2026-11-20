//! `/local/keys/add` — full-page host for [`AddKeyFlow`]: crumbs, a
//! Source → Key → Details stepper, and the flow's Back / Continue bar in the
//! page footer.

use gpui::{div, prelude::*, App, Context, Entity, Global, Subscription, Window};

use super::add_flow::{AddKeyEvent, AddKeyFlow, AddKeyStep};
use super::sources::KeySource;
use crate::router::Page;
use crate::shell::{navigate, toast};
use crate::ui::{page_frame, stepper};

/// Source to open the add page on (set by empty-state CTAs).
struct PendingSource(Option<KeySource>);
impl Global for PendingSource {}

/// Navigate to the add-key page, optionally straight into one source.
pub fn open_add_key(source: Option<KeySource>, cx: &mut App) {
    cx.set_global(PendingSource(source));
    navigate(Page::LocalKeyAdd, cx);
}

pub struct AddKeyPage {
    flow: Entity<AddKeyFlow>,
    _subs: Vec<Subscription>,
}

impl AddKeyPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let flow = cx.new(|cx| {
            let mut f = AddKeyFlow::new(None, window, cx);
            f.set_hosted(true, cx);
            f
        });
        let pending = cx.try_global::<PendingSource>().and_then(|p| p.0);
        if cx.has_global::<PendingSource>() {
            cx.set_global(PendingSource(None));
        }
        if let Some(source) = pending.or_else(dev_source) {
            flow.update(cx, |f, cx| f.choose(source, window, cx));
            if cfg!(debug_assertions) {
                if let Ok(text) = std::env::var("SIGVAULT_DEV_KEY_TEXT") {
                    flow.update(cx, |f, cx| f.dev_submit_text(text, cx));
                }
            }
        }
        let subs = vec![
            cx.observe(&flow, |_, _, cx| cx.notify()),
            cx.subscribe(&flow, |_, _, ev: &AddKeyEvent, cx| match ev {
                AddKeyEvent::Added(id) => {
                    toast(cx, "Key added to your library");
                    navigate(Page::LocalKey(id.clone()), cx);
                }
                AddKeyEvent::Cancelled => navigate(Page::LocalKeys, cx),
            }),
        ];
        Self { flow, _subs: subs }
    }
}

/// Debug builds: `SIGVAULT_DEV_KEY_SOURCE=seed` opens a source directly
/// (screenshots of deep flow states).
fn dev_source() -> Option<KeySource> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("SIGVAULT_DEV_KEY_SOURCE").ok().and_then(|s| KeySource::from_id(&s))
}

impl Render for AddKeyPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let step = self.flow.read(cx).step();
        let (footer, overlay) = self.flow.update(cx, |f, cx| (f.footer(cx), f.overlay(cx)));
        let subtitle = match step {
            AddKeyStep::Source => "Choose where the key comes from. You can add as many keys as you like and combine them into wallets later.",
            AddKeyStep::Key => "Provide the key. Nothing is saved until the last step.",
            AddKeyStep::Details => "Name the key and check its public details before saving it to your library.",
        };
        div()
            .size_full()
            .relative()
            .child(
                page_frame("add-key")
                    .crumb("Key library", Some(Page::LocalKeys))
                    .crumb("Add key", None)
                    .title("Add key")
                    .subtitle(subtitle)
                    .header_extra(stepper(&AddKeyStep::LABELS, step.index()))
                    .footer(footer)
                    .child(self.flow.clone()),
            )
            .children(overlay)
    }
}
