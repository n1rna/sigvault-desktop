//! Bitcoin price card (web `components/bitcoin-price-chart.tsx`).
//!
//! Same data source as the web dashboard: mempool.space's public price API
//! (`/api/v1/prices` for spot, `/api/v1/historical-price?currency=USD` for
//! history). Fetched off the UI thread; failures just leave the card in its
//! empty state — the wallet never depends on it.

use gpui::{
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, App, Bounds, Context,
    PathBuilder, Pixels, SharedString, Window,
};
use serde::Deserialize;

use crate::e2e::{E2eClick, Probe};
use crate::runtime::backend;
use crate::theme::{alpha, theme, FONT_MONO, RADIUS_CARD};
use crate::ui::icon;

const SPOT_URL: &str = "https://mempool.space/api/v1/prices";
const HISTORY_URL: &str = "https://mempool.space/api/v1/historical-price?currency=USD";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Range {
    D1,
    W1,
    M1,
    M3,
    M6,
}

impl Range {
    const ALL: [Range; 5] = [Range::D1, Range::W1, Range::M1, Range::M3, Range::M6];
    fn label(self) -> &'static str {
        match self {
            Range::D1 => "1D",
            Range::W1 => "1W",
            Range::M1 => "1M",
            Range::M3 => "3M",
            Range::M6 => "6M",
        }
    }
    fn secs(self) -> i64 {
        let day = 86_400;
        match self {
            Range::D1 => day,
            Range::W1 => 7 * day,
            Range::M1 => 30 * day,
            Range::M3 => 90 * day,
            Range::M6 => 180 * day,
        }
    }
}

#[derive(Deserialize)]
struct Spot {
    #[serde(rename = "USD")]
    usd: f64,
}

#[derive(Deserialize)]
struct HistoryPoint {
    time: i64,
    #[serde(rename = "USD")]
    usd: f64,
}

#[derive(Deserialize)]
struct History {
    prices: Vec<HistoryPoint>,
}

pub struct PriceChart {
    range: Range,
    spot: Option<f64>,
    history: Vec<(i64, f64)>,
    loading: bool,
    failed: bool,
}

impl PriceChart {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let task = backend(cx).spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .ok()?;
            let spot = async {
                client
                    .get(SPOT_URL)
                    .send()
                    .await
                    .ok()?
                    .json::<Spot>()
                    .await
                    .ok()
            };
            let hist = async {
                client
                    .get(HISTORY_URL)
                    .send()
                    .await
                    .ok()?
                    .json::<History>()
                    .await
                    .ok()
            };
            let (spot, hist) = futures::join!(spot, hist);
            Some((
                spot.map(|s| s.usd),
                hist.map(|h| h.prices).unwrap_or_default(),
            ))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await.flatten();
            this.update(cx, |this, cx| {
                this.loading = false;
                match res {
                    Some((spot, mut hist)) => {
                        hist.sort_by_key(|p| p.time);
                        this.history = hist
                            .into_iter()
                            .filter(|p| p.usd > 0.)
                            .map(|p| (p.time, p.usd))
                            .collect();
                        this.spot = spot.or_else(|| this.history.last().map(|p| p.1));
                        this.failed = this.spot.is_none();
                    }
                    None => this.failed = true,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            range: Range::M1,
            spot: None,
            history: Vec::new(),
            loading: true,
            failed: false,
        }
    }

    fn series(&self) -> Vec<(i64, f64)> {
        let Some(last) = self.history.last().map(|p| p.0) else {
            return Vec::new();
        };
        let from = last - self.range.secs();
        let mut pts: Vec<(i64, f64)> = self
            .history
            .iter()
            .copied()
            .filter(|p| p.0 >= from)
            .collect();
        if let (Some(spot), Some(&(t, _))) = (self.spot, pts.last()) {
            pts.push((t.max(chrono::Utc::now().timestamp()), spot));
        }
        pts
    }
}

fn fmt_usd(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("${out}")
}

fn fmt_day(ts: i64, range: Range) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts, 0) {
        chrono::LocalResult::Single(dt) if range == Range::D1 => dt.format("%H:%M").to_string(),
        chrono::LocalResult::Single(dt) => dt.format("%b %-d").to_string(),
        _ => String::new(),
    }
}

impl Render for PriceChart {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme();
        let series = self.series();
        let change = match (series.first(), series.last()) {
            (Some(a), Some(b)) if a.1 > 0. => Some((b.1 - a.1) / a.1 * 100.),
            _ => None,
        };
        let (lo, hi) = series.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
        // Nice y-axis bounds: pad 5% and round to 4 ticks.
        // Round ticks: step from {1, 2, 2.5, 5} × 10^k, ~4 intervals.
        let (y0, y1, step) = if series.is_empty() {
            (0., 1., 0.25)
        } else {
            let raw = ((hi - lo).max(hi * 0.01)) / 4.;
            let mag = 10f64.powf(raw.log10().floor());
            let step = [1., 2., 2.5, 5., 10.]
                .iter()
                .map(|m| m * mag)
                .find(|s| *s >= raw)
                .unwrap_or(10. * mag);
            ((lo / step).floor() * step, (hi / step).ceil() * step, step)
        };
        let n_ticks = (((y1 - y0) / step).round() as usize).max(1);
        let ticks: Vec<f64> = (0..=n_ticks).rev().map(|i| y0 + step * i as f64).collect();
        let x_labels: Vec<String> = if series.len() > 1 {
            let (a, b) = (series[0].0, series[series.len() - 1].0);
            (0..=6)
                .map(|i| fmt_day(a + (b - a) * i / 6, self.range))
                .collect()
        } else {
            Vec::new()
        };

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .px_6()
            .py_5()
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(crate::ui::eyebrow("Bitcoin price"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_baseline()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .line_height(px(34.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(match (self.spot, self.loading) {
                                        (Some(p), _) => fmt_usd(p),
                                        (None, true) => "…".into(),
                                        (None, false) => "—".into(),
                                    }),
                            )
                            .children(change.map(|c| {
                                let color = if c >= 0. { t.success } else { t.destructive };
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .text_size(px(14.))
                                    .text_color(color)
                                    .child(
                                        icon(if c >= 0. {
                                            "trending-up"
                                        } else {
                                            "trending-down"
                                        })
                                        .size(px(15.))
                                        .text_color(color),
                                    )
                                    .child(format!(
                                        "{}{:.1}% ({})",
                                        if c >= 0. { "+" } else { "" },
                                        c,
                                        self.range.label()
                                    ))
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .p(px(3.))
                    .gap(px(2.))
                    .rounded_full()
                    .bg(t.inset)
                    .children(Range::ALL.into_iter().map(|r| {
                        let active = r == self.range;
                        div()
                            .id(r.label())
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .cursor_pointer()
                            .text_color(if active { t.ink } else { t.ink_3 })
                            .when(active, |d| d.bg(t.page).border_1().border_color(t.line))
                            .when(!active, |d| d.hover(|s| s.text_color(t.ink)))
                            .clickable(
                                Probe::new("tab", r.label()).selected(active),
                                cx.listener(move |this, _, _, cx| {
                                    this.range = r;
                                    cx.notify();
                                }),
                            )
                            .child(r.label())
                    })),
            );

        let plot_h = px(300.);
        let body = if series.len() < 2 {
            div()
                .h(plot_h)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(t.ink_3)
                .child(if self.failed && !self.loading {
                    SharedString::from("Price data unavailable (mempool.space unreachable)")
                } else {
                    SharedString::from("Loading price history…")
                })
                .into_any_element()
        } else {
            let pts = series.clone();
            let line = t.brand;
            let plot = canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, _, window: &mut Window, _cx: &mut App| {
                    let (t0, t1) = (pts[0].0 as f64, pts[pts.len() - 1].0 as f64);
                    let w = f32::from(bounds.size.width) as f64;
                    let h = f32::from(bounds.size.height) as f64;
                    let at = |p: &(i64, f64)| {
                        let x = (p.0 as f64 - t0) / (t1 - t0).max(1.) * w;
                        let y = (1. - (p.1 - y0) / (y1 - y0)) * h;
                        point(bounds.left() + px(x as f32), bounds.top() + px(y as f32))
                    };
                    // Area fill with a top-down fade, then the line on top.
                    let mut fill = PathBuilder::fill();
                    fill.move_to(point(bounds.left(), bounds.bottom()));
                    for p in &pts {
                        fill.line_to(at(p));
                    }
                    fill.line_to(point(at(&pts[pts.len() - 1]).x, bounds.bottom()));
                    fill.close();
                    if let Ok(path) = fill.build() {
                        window.paint_path(
                            path,
                            linear_gradient(
                                180.,
                                linear_color_stop(alpha(line, 0.28), 0.),
                                linear_color_stop(alpha(line, 0.0), 1.),
                            ),
                        );
                    }
                    let mut stroke = PathBuilder::stroke(px(1.6));
                    stroke.move_to(at(&pts[0]));
                    for p in &pts[1..] {
                        stroke.line_to(at(p));
                    }
                    if let Ok(path) = stroke.build() {
                        window.paint_path(path, line);
                    }
                },
            )
            .flex_1()
            .h_full();

            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .w_full()
                        .h(plot_h)
                        .flex()
                        .flex_row()
                        .gap_3()
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .flex()
                                .flex_col()
                                .justify_between()
                                .items_end()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .text_color(t.ink_2)
                                .children(ticks.iter().map(|v| div().child(fmt_usd(*v)))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .flex()
                                .flex_row()
                                .border_l_1()
                                .border_b_1()
                                .border_color(t.line)
                                .child(plot),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .pl(px(76.))
                        .flex()
                        .flex_row()
                        .justify_between()
                        .font_family(FONT_MONO)
                        .text_size(px(12.))
                        .text_color(t.ink_2)
                        .children(x_labels.into_iter().map(|l| div().child(l))),
                )
                .into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(RADIUS_CARD)
            .border_1()
            .border_color(t.line)
            .bg(t.page)
            .overflow_hidden()
            .child(header)
            .child(div().w_full().flex().flex_col().px_6().py_6().child(body))
    }
}
