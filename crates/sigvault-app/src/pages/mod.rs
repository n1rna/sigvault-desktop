//! One module per route in `App.tsx`. Each page is a GPUI entity built
//! fresh on navigation (see [`build`]).

pub mod cloud;
pub mod home;
pub mod loading;
pub mod local;
pub mod login;
pub mod select_env;
pub mod welcome;

use gpui::{AnyView, App, AppContext, Window};

use crate::router::Page;

pub fn build(page: &Page, window: &mut Window, cx: &mut App) -> AnyView {
    match page.clone() {
        Page::Loading => cx.new(|cx| loading::LoadingPage::new(window, cx)).into(),
        Page::Welcome => cx.new(|cx| welcome::WelcomePage::new(window, cx)).into(),
        Page::SelectEnv => cx.new(|cx| select_env::SelectEnvPage::new(window, cx)).into(),
        Page::Login => cx.new(|cx| login::LoginPage::new(window, cx)).into(),
        Page::Register => cx.new(|cx| cloud::register::MachineRegistrationPage::new(window, cx)).into(),
        Page::Sessions => cx.new(|cx| cloud::sessions::RemoteSessionsPage::new(window, cx)).into(),
        Page::CloudSettings => cx.new(|cx| cloud::settings::CloudSettingsPage::new(window, cx)).into(),
        Page::SessionDetails => cx.new(|cx| cloud::session_details::SessionDetailsPage::new(window, cx)).into(),
        Page::LocalHome => cx.new(|cx| home::HomePage::new(sigvault_core::app_mode::AppMode::Local, window, cx)).into(),
        Page::CloudHome => cx.new(|cx| home::HomePage::new(sigvault_core::app_mode::AppMode::Cloud, window, cx)).into(),
        Page::LocalWallets => cx.new(|cx| local::wallet_list::LocalWalletListPage::new(window, cx)).into(),
        Page::LocalWalletNew => cx.new(|cx| local::create_wizard::CreateWalletWizardPage::new(window, cx)).into(),
        Page::LocalWalletRecover => cx.new(|cx| local::recovery::RecoveryFlowPage::new(window, cx)).into(),
        Page::LocalWallet(id) => cx.new(|cx| local::dashboard::WalletDashboardPage::new(id, window, cx)).into(),
        Page::LocalReceive(id) => cx.new(|cx| local::receive::ReceivePage::new(id, window, cx)).into(),
        Page::LocalSend(id) => cx.new(|cx| local::send::SendPage::new(id, window, cx)).into(),
        Page::LocalKeys => cx.new(|cx| local::keys::list::KeyListPage::new(window, cx)).into(),
        Page::LocalKeyAdd => cx.new(|cx| local::keys::add::AddKeyPage::new(window, cx)).into(),
        Page::LocalKey(id) => cx.new(|cx| local::keys::detail::KeyDetailPage::new(id, window, cx)).into(),
        Page::LocalSettings => cx.new(|cx| local::settings::LocalSettingsPage::new(window, cx)).into(),
    }
}
