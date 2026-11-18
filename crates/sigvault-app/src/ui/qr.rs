use gpui::{canvas, div, fill, point, prelude::*, px, size, App, Bounds, Pixels, Window};
use qrcode::{Color, EcLevel};

use crate::theme::theme;

/// Static QR code. Dark modules on a light quiet-zone tile so phones and
/// hardware-wallet cameras can read it on the dark theme.
#[derive(IntoElement)]
pub struct QrCode {
    data: String,
    size: Pixels,
}

impl QrCode {
    pub fn new(data: impl Into<String>, size: Pixels) -> Self {
        Self { data: data.into(), size }
    }
}

impl RenderOnce for QrCode {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let t = theme();
        let code = qrcode::QrCode::with_error_correction_level(self.data.as_bytes(), EcLevel::L)
            .or_else(|_| qrcode::QrCode::new(self.data.as_bytes()));
        let (modules, width) = match code {
            Ok(c) => {
                let w = c.width();
                (c.to_colors(), w)
            }
            Err(_) => (Vec::new(), 0),
        };
        let fg = t.background;
        div()
            .size(self.size)
            .flex_none()
            .rounded_md()
            .bg(t.foreground)
            .p(px(10.))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window: &mut Window, _cx: &mut App| {
                        if width == 0 {
                            return;
                        }
                        let cell = bounds.size.width.min(bounds.size.height) / width as f32;
                        for (i, c) in modules.iter().enumerate() {
                            if *c == Color::Dark {
                                let x = (i % width) as f32;
                                let y = (i / width) as f32;
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left() + cell * x, bounds.top() + cell * y),
                                        size(cell + px(0.3), cell + px(0.3)),
                                    ),
                                    fg,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}
