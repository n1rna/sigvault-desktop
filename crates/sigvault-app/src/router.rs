//! Client-side routing, mirroring `App.tsx`.
//!
//! The backend drives a coarse `WindowApplicationRoute`; the UI may drill
//! into child pages (`/local/wallets/<id>/send`) as long as they stay inside
//! the backend route's target or namespace. Any navigation outside it snaps
//! back to the route's default page — the same rule the React router effect
//! enforced on every location change.

use sigvault_core::window::WindowApplicationRoute as R;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Loading,
    Welcome,
    SelectEnv,
    Login,
    LocalWallets,
    LocalWalletNew,
    LocalWalletRecover,
    LocalWallet(String),
    LocalReceive(String),
    LocalSend(String),
    LocalSettings,
    Register,
    Sessions,
    SessionDetails,
}

impl Page {
    pub fn path(&self) -> String {
        match self {
            Page::Loading => "/".into(),
            Page::Welcome => "/welcome".into(),
            Page::SelectEnv => "/select-env".into(),
            Page::Login => "/login".into(),
            Page::LocalWallets => "/local/wallets".into(),
            Page::LocalWalletNew => "/local/wallets/new".into(),
            Page::LocalWalletRecover => "/local/wallets/recover".into(),
            Page::LocalWallet(id) => format!("/local/wallets/{id}"),
            Page::LocalReceive(id) => format!("/local/wallets/{id}/receive"),
            Page::LocalSend(id) => format!("/local/wallets/{id}/send"),
            Page::LocalSettings => "/local/settings".into(),
            Page::Register => "/register".into(),
            Page::Sessions => "/sessions".into(),
            Page::SessionDetails => "/session-details".into(),
        }
    }
}

struct RouteSpec {
    target: Page,
    namespace: Option<&'static str>,
}

fn spec(route: &R) -> RouteSpec {
    let (target, namespace) = match route {
        R::Loading => (Page::Loading, None),
        R::Welcome => (Page::Welcome, None),
        R::SelectEnv => (Page::SelectEnv, None),
        R::Login => (Page::Login, None),
        R::MainPage => (Page::Sessions, None),
        R::MachineRegistration => (Page::Register, None),
        R::RemoteSessions => (Page::Sessions, None),
        R::SessionDetails => (Page::SessionDetails, None),
        R::LocalWallets => (Page::LocalWallets, Some("/local/")),
    };
    RouteSpec { target, namespace }
}

/// Given the backend route and the page the UI wants to show, return the
/// page that should actually be shown.
pub fn resolve(route: &R, requested: &Page) -> Page {
    let RouteSpec { target, namespace } = spec(route);
    let path = requested.path();
    let target_path = target.path();
    if path == target_path {
        return requested.clone();
    }
    if target_path != "/" && path.starts_with(&format!("{target_path}/")) {
        return requested.clone();
    }
    if let Some(ns) = namespace {
        if path.starts_with(ns) {
            return requested.clone();
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_namespace_keeps_siblings() {
        assert_eq!(resolve(&R::LocalWallets, &Page::LocalSettings), Page::LocalSettings);
        assert_eq!(
            resolve(&R::LocalWallets, &Page::LocalSend("x".into())),
            Page::LocalSend("x".into())
        );
    }

    #[test]
    fn leaving_namespace_snaps_back() {
        assert_eq!(resolve(&R::LocalWallets, &Page::Sessions), Page::LocalWallets);
        assert_eq!(resolve(&R::Login, &Page::LocalWallets), Page::Login);
        assert_eq!(resolve(&R::MainPage, &Page::Login), Page::Sessions);
    }

    #[test]
    fn loading_root_does_not_capture_everything() {
        assert_eq!(resolve(&R::Loading, &Page::Welcome), Page::Loading);
    }
}
