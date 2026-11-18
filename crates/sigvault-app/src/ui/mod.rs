//! Component kit mirroring the Tauri build's shadcn-style primitives
//! (`components/ui/*`) and recurring page patterns.
#![allow(unused_imports)]

pub mod button;
pub mod common;
pub mod input;
pub mod modal;
pub mod qr;
pub mod select;

pub use button::{button, icon_button, Button, ButtonSize, ButtonVariant};
pub use common::*;
pub use input::{InputEvent, TextInput};
pub use modal::modal;
pub use qr::QrCode;
pub use select::{Select, SelectEvent, SelectOption};
