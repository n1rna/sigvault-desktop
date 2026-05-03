//! In-memory state for unlocked local wallets.
//!
//! Holds one `UnlockedHandle` per wallet that the user has decrypted with
//! a passphrase. Locking a wallet drops its handle, which (via Drop on
//! `Zeroizing`) wipes the seed bytes from memory before deallocation.
//!
//! Each handle is wrapped in `Arc<Mutex<...>>` so a long-running
//! operation on wallet A (e.g. an Electrum full_scan that takes 30s)
//! doesn't block reads on wallet B. The outer `Mutex<HashMap<...>>` is
//! only held briefly to look up / clone the per-wallet `Arc`.

use std::collections::HashMap;
use std::sync::Arc;

use bdk_wallet::PersistedWallet;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::persister::LocalBdkPersister;
use super::storage::WalletId;

/// A live, decrypted wallet plus its persister and the seed bytes the
/// user typed the passphrase to recover. The seed is wrapped in
/// `Zeroizing` so the bytes are wiped when the handle is dropped.
pub struct UnlockedHandle {
    pub wallet: PersistedWallet<LocalBdkPersister>,
    pub persister: LocalBdkPersister,
    /// BIP39 mnemonic words (UTF-8). Wrapped in `Zeroizing` so locking
    /// the wallet (which drops this handle) wipes the seed material from
    /// memory before deallocation.
    pub mnemonic: Zeroizing<Vec<u8>>,
}

/// Per-wallet handle pointer. The inner `Mutex` serialises operations
/// on a single wallet; the outer map's Mutex is released as soon as the
/// pointer is cloned, so other wallets' operations are not blocked.
pub type SharedHandle = Arc<Mutex<UnlockedHandle>>;

#[derive(Default)]
pub struct LocalWalletState {
    unlocked: Mutex<HashMap<WalletId, SharedHandle>>,
}

impl LocalWalletState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a handle. Returns the previous handle if any (caller can
    /// decide what to do with it — usually drop, which Zeroize-wipes).
    pub async fn insert(&self, id: WalletId, handle: UnlockedHandle) -> Option<SharedHandle> {
        self.unlocked
            .lock()
            .await
            .insert(id, Arc::new(Mutex::new(handle)))
    }

    /// Look up a handle by id and return a cheap pointer clone. The
    /// caller then locks the per-wallet Mutex for the duration of its
    /// operation. Returns `None` for locked / missing wallets.
    pub async fn get(&self, id: &WalletId) -> Option<SharedHandle> {
        self.unlocked.lock().await.get(id).cloned()
    }

    /// Lock a wallet by removing its handle from the map. Drop semantics
    /// on `UnlockedHandle.mnemonic` (Zeroizing) wipe the seed bytes
    /// once all outstanding `SharedHandle` clones go out of scope.
    /// Returns true if the wallet was previously unlocked.
    pub async fn lock_wallet(&self, id: &WalletId) -> bool {
        self.unlocked.lock().await.remove(id).is_some()
    }

    pub async fn is_unlocked(&self, id: &WalletId) -> bool {
        self.unlocked.lock().await.contains_key(id)
    }

    /// Remove all handles. Used for full sign-out / mode switch.
    pub async fn lock_all(&self) {
        self.unlocked.lock().await.clear();
    }
}

/// Convenience type alias for the shared state pointer threaded through
/// `ApplicationState` and the `LocalWalletManager`.
pub type SharedLocalWalletState = Arc<LocalWalletState>;
