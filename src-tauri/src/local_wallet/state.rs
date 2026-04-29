//! In-memory state for unlocked local wallets.
//!
//! Holds one `UnlockedHandle` per wallet that the user has decrypted with
//! a passphrase. Locking a wallet drops its handle, which (via Drop on
//! `Zeroizing`) wipes the seed bytes from memory before deallocation.
//!
//! The map is guarded by an async `Mutex` so commands can hold the lock
//! across `.await` points (e.g. when waiting on BDK's `wallet.persist()`).

use std::collections::HashMap;
use std::sync::Arc;

use bdk_wallet::PersistedWallet;
use tokio::sync::{Mutex, MutexGuard};
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

#[derive(Default)]
pub struct LocalWalletState {
    unlocked: Mutex<HashMap<WalletId, UnlockedHandle>>,
}

impl LocalWalletState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lock-acquire helper. Holds the mutex for the duration of the
    /// returned guard; callers who need to await across the access
    /// should pass the guard around rather than re-acquire.
    pub async fn lock(&self) -> MutexGuard<'_, HashMap<WalletId, UnlockedHandle>> {
        self.unlocked.lock().await
    }

    pub async fn insert(&self, id: WalletId, handle: UnlockedHandle) {
        self.unlocked.lock().await.insert(id, handle);
    }

    /// Lock a wallet by removing its handle from the map. Drop semantics
    /// on `UnlockedHandle.mnemonic` (Zeroizing) wipe the seed bytes.
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
