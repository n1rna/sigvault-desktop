//! Key library pages (local mode → Keys).
//!
//! - [`list::KeyListPage`]       `/local/keys`        all keys, lock state, usage
//! - [`add::AddKeyPage`]         `/local/keys/add`    page host for [`add_flow::AddKeyFlow`]
//! - [`detail::KeyDetailPage`]   `/local/keys/<id>`   xpub, QR, export, rename, delete
//! - [`add_flow::AddKeyFlow`]    embeddable add/import flow (also used in a
//!   modal by the wallet wizard's key picker)
//! - [`unlock::KeychainUnlock`]  password dialog (unlock or set keychain password)

pub mod add;
pub mod add_flow;
pub mod detail;
pub mod list;
pub mod sources;
pub mod unlock;
