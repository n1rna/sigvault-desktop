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
    /// "On this device" dashboard.
    LocalHome,
    LocalWallets,
    LocalWalletNew,
    LocalWalletRecover,
    LocalWallet(String),
    LocalReceive(String),
    LocalSend(String),
    LocalSettings,
    /// Key library.
    LocalKeys,
    /// Add / import a key into the library.
    LocalKeyAdd,
    /// Key detail.
    LocalKey(String),
    Register,
    Sessions,
    SessionDetails,
    /// Cloud space settings: environment, account, about.
    CloudSettings,
    /// "SigVault Cloud" dashboard.
    CloudHome,
}

impl Page {
    pub fn path(&self) -> String {
        match self {
            Page::Loading => "/".into(),
            Page::Welcome => "/welcome".into(),
            Page::SelectEnv => "/cloud/select-env".into(),
            Page::Login => "/cloud/login".into(),
            Page::LocalHome => "/local/home".into(),
            Page::LocalWallets => "/local/wallets".into(),
            Page::LocalWalletNew => "/local/wallets/new".into(),
            Page::LocalWalletRecover => "/local/wallets/recover".into(),
            Page::LocalWallet(id) => format!("/local/wallets/{id}"),
            Page::LocalReceive(id) => format!("/local/wallets/{id}/receive"),
            Page::LocalSend(id) => format!("/local/wallets/{id}/send"),
            Page::LocalSettings => "/local/settings".into(),
            Page::LocalKeys => "/local/keys".into(),
            Page::LocalKeyAdd => "/local/keys/add".into(),
            Page::LocalKey(id) => format!("/local/keys/{id}"),
            Page::Register => "/cloud/register".into(),
            Page::Sessions => "/cloud/sessions".into(),
            Page::SessionDetails => "/cloud/session".into(),
            Page::CloudSettings => "/cloud/settings".into(),
            Page::CloudHome => "/cloud/home".into(),
        }
    }
}

impl Page {
    /// Inverse of [`Page::path`] (used by the `SIGVAULT_DEV_PAGE` hook).
    pub fn from_path(path: &str) -> Option<Page> {
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        Some(match parts.as_slice() {
            [""] => Page::Loading,
            ["welcome"] => Page::Welcome,
            ["cloud", "select-env"] => Page::SelectEnv,
            ["cloud", "login"] => Page::Login,
            ["cloud", "register"] => Page::Register,
            ["cloud", "sessions"] => Page::Sessions,
            ["cloud", "session"] => Page::SessionDetails,
            ["cloud", "settings"] => Page::CloudSettings,
            ["cloud", "home"] => Page::CloudHome,
            ["local", "home"] => Page::LocalHome,
            ["local", "wallets"] => Page::LocalWallets,
            ["local", "wallets", "new"] => Page::LocalWalletNew,
            ["local", "wallets", "recover"] => Page::LocalWalletRecover,
            ["local", "wallets", id] => Page::LocalWallet(id.to_string()),
            ["local", "wallets", id, "receive"] => Page::LocalReceive(id.to_string()),
            ["local", "wallets", id, "send"] => Page::LocalSend(id.to_string()),
            ["local", "settings"] => Page::LocalSettings,
            ["local", "keys"] => Page::LocalKeys,
            ["local", "keys", "add"] => Page::LocalKeyAdd,
            ["local", "keys", id] => Page::LocalKey(id.to_string()),
            _ => return None,
        })
    }

    /// Pages in the "On this device" space.
    pub fn is_local(&self) -> bool {
        self.path().starts_with("/local/")
    }

    /// Pages in the "SigVault Cloud" space.
    pub fn is_cloud(&self) -> bool {
        self.path().starts_with("/cloud/")
    }

    /// Pages rendered inside the sidebar layout (everything except the
    /// first-launch Welcome and boot Loading screens).
    pub fn has_sidebar(&self) -> bool {
        self.is_local() || self.is_cloud()
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
        // Signed in: any /cloud/ page (settings, register) is fine.
        R::MainPage => (Page::CloudHome, Some("/cloud/")),
        R::MachineRegistration => (Page::Register, None),
        R::RemoteSessions => (Page::Sessions, Some("/cloud/")),
        R::SessionDetails => (Page::SessionDetails, None),
        R::LocalWallets => (Page::LocalHome, Some("/local/")),
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
    // The session page shows the core's active session; once the core routes
    // away (session finished or left), it must not linger under the /cloud/
    // namespace.
    if *requested == Page::SessionDetails {
        return target;
    }
    if let Some(ns) = namespace {
        // Signed-in cloud routes never fall back to the pre-auth pages.
        let pre_auth = matches!(requested, Page::Login | Page::SelectEnv);
        let signed_in_route = matches!(route, R::MainPage | R::RemoteSessions);
        if path.starts_with(ns) && !(pre_auth && signed_in_route) {
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
        assert_eq!(
            resolve(&R::LocalWallets, &Page::LocalSettings),
            Page::LocalSettings
        );
        assert_eq!(
            resolve(&R::LocalWallets, &Page::LocalSend("x".into())),
            Page::LocalSend("x".into())
        );
    }

    #[test]
    fn leaving_namespace_snaps_back() {
        assert_eq!(resolve(&R::LocalWallets, &Page::Sessions), Page::LocalHome);
        assert_eq!(resolve(&R::Login, &Page::LocalWallets), Page::Login);
        assert_eq!(resolve(&R::MainPage, &Page::Login), Page::CloudHome);
        assert_eq!(
            resolve(&R::MainPage, &Page::CloudSettings),
            Page::CloudSettings
        );
        assert_eq!(resolve(&R::Login, &Page::Sessions), Page::Login);
    }

    #[test]
    fn finished_session_returns_to_the_list() {
        assert_eq!(
            resolve(&R::RemoteSessions, &Page::SessionDetails),
            Page::Sessions
        );
        assert_eq!(
            resolve(&R::MainPage, &Page::SessionDetails),
            Page::CloudHome
        );
        assert_eq!(
            resolve(&R::SessionDetails, &Page::SessionDetails),
            Page::SessionDetails
        );
    }

    #[test]
    fn paths_round_trip() {
        for p in [
            Page::LocalKeys,
            Page::LocalKeyAdd,
            Page::LocalKey("k1".into()),
            Page::LocalSend("w".into()),
            Page::Sessions,
            Page::CloudSettings,
            Page::SessionDetails,
        ] {
            assert_eq!(Page::from_path(&p.path()), Some(p));
        }
    }

    #[test]
    fn loading_root_does_not_capture_everything() {
        assert_eq!(resolve(&R::Loading, &Page::Welcome), Page::Loading);
    }
}
