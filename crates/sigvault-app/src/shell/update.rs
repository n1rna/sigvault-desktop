//! Update banner (`components/UpdateBanner.tsx`).
//!
//! The Tauri build used the signed Tauri updater. This build checks the
//! same release feed (`latest.json`) and, when a newer version exists,
//! offers to open the release page — installers are per-platform bundles,
//! so the download itself happens in the browser.

use gpui::{div, prelude::*, px, Context, SharedString, Window};
use serde::Deserialize;

use crate::runtime::backend;
use crate::theme::{alpha, theme};
use crate::ui::{badge, button, Tone};

const FEED_URL: &str =
    "https://github.com/n1rna/sigvault-desktop/releases/latest/download/latest.json";
const RELEASES_URL: &str = "https://github.com/n1rna/sigvault-desktop/releases/latest";

#[derive(Deserialize)]
struct Feed {
    version: String,
}

pub struct UpdateBanner {
    available: Option<SharedString>,
    dismissed: bool,
}

fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| semver::Version::parse(s.trim_start_matches('v')).ok();
    match (parse(remote), parse(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

impl UpdateBanner {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let local = env!("CARGO_PKG_VERSION");
        // Development builds (0.0.0-dev) never nag.
        if !local.ends_with("-dev") && std::env::var_os("SIGVAULT_NO_UPDATE_CHECK").is_none() {
            let task = backend(cx).spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .ok()?;
                let feed: Feed = client.get(FEED_URL).send().await.ok()?.json().await.ok()?;
                is_newer(&feed.version, local).then_some(feed.version)
            });
            cx.spawn(async move |this, cx| {
                if let Some(Some(version)) = task.await {
                    this.update(cx, |this, cx| {
                        this.available = Some(version.into());
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }
        Self {
            available: None,
            dismissed: false,
        }
    }
}

impl Render for UpdateBanner {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let Some(version) = self.available.clone().filter(|_| !self.dismissed) else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .bottom(px(52.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .px_4()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_w(px(360.))
                    .rounded_full()
                    .border_1()
                    .border_color(t.border)
                    .bg(alpha(t.card, 0.95))
                    .shadow_lg()
                    .py_2()
                    .pl_4()
                    .pr_2()
                    .child(div().size(px(8.)).rounded_full().bg(t.primary))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .child(badge("Update available", Tone::Default))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("v{version}")),
                            ),
                    )
                    .child(
                        button("update-dismiss", "Later")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismissed = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("update-open", "Download")
                            .small()
                            .icon("download")
                            .on_click(|_, _, cx| cx.open_url(RELEASES_URL)),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_compare() {
        assert!(is_newer("v1.2.0", "1.1.9"));
        assert!(!is_newer("1.1.9", "1.1.9"));
        assert!(!is_newer("garbage", "1.0.0"));
    }
}
