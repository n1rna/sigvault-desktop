//! Top-level application mode (Cloud vs Local).
//!
//! Cloud mode is the existing remote-signing experience: pick a SigVault
//! environment, sign in, mediate device interactions for cloud-coordinated
//! sessions. Local mode is the new fully-offline Bitcoin wallet experience:
//! no cloud account, wallets stored encrypted on disk, transactions signed
//! and broadcast directly.
//!
//! The selection is persisted alongside the chosen environment in
//! `EnvStorage`; a fresh install (or one whose mode was explicitly cleared
//! via "Switch mode" in settings) returns `None` here, which the
//! `cmd_initialize_app` handler interprets as "show the mode chooser".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Cloud,
    Local,
}

impl AppMode {
    /// Convenience predicate. Public so QBL-215+ local-mode storage code
    /// can branch on it; left here pre-emptively rather than re-adding
    /// once those modules land.
    #[allow(dead_code)]
    pub fn is_cloud(self) -> bool {
        matches!(self, AppMode::Cloud)
    }

    #[allow(dead_code)]
    pub fn is_local(self) -> bool {
        matches!(self, AppMode::Local)
    }
}
