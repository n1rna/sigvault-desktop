//! BDK changeset persister for local wallets.
//!
//! Wraps `bdk_file_store::Store` so the BDK 1.x changeset log lives in the
//! per-wallet directory at `bdk_store`. The seed itself is stored
//! separately in `seed.enc` (encrypted) — `bdk_store` holds keychain
//! indexes, transaction graph, and chain anchor data, all of which are
//! privacy-sensitive but cannot be used to spend on their own.

use std::path::Path;

use bdk_file_store::Store;
use thiserror::Error;
use wallet_runtime::{ChangeSet, WalletPersister};

const BDK_STORE_MAGIC: &[u8] = b"sigvault-local-bdk-v1";

#[derive(Debug, Error)]
pub enum LocalPersisterError {
    #[error("file_store error: {0}")]
    Store(String),
}

pub struct LocalBdkPersister {
    inner: Store<ChangeSet>,
}

impl LocalBdkPersister {
    /// Open or create a BDK store at the given path. Idempotent — used
    /// both at wallet-creation time (fresh store) and unlock time (reload
    /// existing store).
    pub fn open_or_create(path: &Path) -> Result<Self, LocalPersisterError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| LocalPersisterError::Store(format!("mkdir {parent:?}: {e}")))?;
        }
        let inner = Store::<ChangeSet>::open_or_create_new(BDK_STORE_MAGIC, path)
            .map_err(|e| LocalPersisterError::Store(format!("open store: {e}")))?;
        Ok(Self { inner })
    }
}

impl WalletPersister for LocalBdkPersister {
    type Error = LocalPersisterError;

    fn initialize(p: &mut Self) -> Result<ChangeSet, Self::Error> {
        <Store<ChangeSet> as WalletPersister>::initialize(&mut p.inner)
            .map_err(|e| LocalPersisterError::Store(format!("initialize: {e}")))
    }

    fn persist(p: &mut Self, changeset: &ChangeSet) -> Result<(), Self::Error> {
        <Store<ChangeSet> as WalletPersister>::persist(&mut p.inner, changeset)
            .map_err(|e| LocalPersisterError::Store(format!("persist: {e}")))
    }
}
