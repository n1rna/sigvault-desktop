//! Shared pieces of the create and recover wizards (`pages/local/setup/*`
//! in the Tauri build): the slot-based policy model, key helpers, the
//! backend wrappers, and the step components both flows reuse.

pub mod api;
pub mod device_picker;
pub mod files;
pub mod finish;
pub mod key_backup;
pub mod keys;
pub mod mnemonic;
pub mod policy;
pub mod policy_picker;
pub mod slot_board;
pub mod ui;
