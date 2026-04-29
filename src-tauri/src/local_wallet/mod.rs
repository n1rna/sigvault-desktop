//! Local-wallet (standalone, offline) support.
//!
//! This module owns the per-wallet directory layout under the app's data
//! dir, the passphrase-based seed encryption envelope, and the metadata /
//! descriptor sidecars. Higher layers — the wallet manager (QBL-216) that
//! drives `wallet-runtime`, the sync engine (QBL-218), the PSBT pipeline
//! (QBL-219), and the Tauri command surface — sit on top of this.

// QBL-215 lays the storage primitives. Consumers (QBL-216 manager, QBL-219
// PSBT pipeline, etc.) wire them up; until those land, every public item
// here looks "unused" to the dead-code analyser. The blanket allow is the
// most readable way to say "this is API surface, not orphaned code".
#![allow(dead_code)]

pub mod storage;
