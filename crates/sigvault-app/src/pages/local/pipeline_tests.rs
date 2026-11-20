//! End-to-end check of the key-library → wallet pipeline without the UI:
//! store a private key the way `Keychain::add_hot` does, read it back the way
//! `Keychain::secret` does, shape it into wizard `WireSlot`s, push it through
//! the exact JSON `api::create_from_slots` sends, and have the real core
//! create + unlock the wallet and derive an address.

use bdk_wallet::KeychainKind;
use serde_json::json;
use sigvault_core::local_wallet::commands::CreateFromSlotsRequest;
use sigvault_core::local_wallet::manager::LocalWalletManager;
use sigvault_core::local_wallet::slots;
use sigvault_core::local_wallet::state::LocalWalletState;
use sigvault_core::local_wallet::storage::{decrypt_seed, encrypt_seed, HotSecret as CoreSecret};

use super::setup::api::{HotSecret, SlotKey, SlotSource, WireSlot};
use super::setup::policy::{layout_for_policy, to_wire_policy, PolicyDraft};

const NET: bitcoin::Network = bitcoin::Network::Regtest;

/// A library record: public half + secret encrypted under the keychain password.
fn library_key(
    path: &str,
    password: &str,
) -> (SlotKey, sigvault_core::local_wallet::storage::EncryptedSeed) {
    let words = slots::generate_mnemonic(12).unwrap().join(" ");
    let secret = CoreSecret::Mnemonic {
        mnemonic: words,
        bip39_passphrase: String::new(),
    };
    let resolved = slots::resolve_secret(NET, &secret, Some(path), None, None).unwrap();
    let d = slots::DerivedKey::from(&resolved);
    let env = encrypt_seed(&serde_json::to_vec(&secret).unwrap(), password.as_bytes()).unwrap();
    (
        SlotKey {
            fingerprint: d.fingerprint,
            derivation_path: d.derivation_path,
            xpub: d.xpub,
        },
        env,
    )
}

/// What the wizard does after `Keychain::secret`.
fn open(env: &sigvault_core::local_wallet::storage::EncryptedSeed, password: &str) -> HotSecret {
    let plain = decrypt_seed(env, password.as_bytes()).expect("keychain password");
    let core: CoreSecret = serde_json::from_slice(&plain).unwrap();
    HotSecret::from(&core)
}

async fn create(
    dir: &std::path::Path,
    policy: PolicyDraft,
    slots: Vec<WireSlot>,
    pass: Option<&str>,
) -> String {
    let layout = layout_for_policy(&policy, "regtest");
    let body = json!({
        "name": "Pipeline",
        "network": "regtest",
        "policy": to_wire_policy(&policy, &layout),
        "slots": slots,
        "encrypt_passphrase": pass,
    });
    let req: CreateFromSlotsRequest =
        serde_json::from_value(body).expect("core accepts wizard JSON");
    let mgr = LocalWalletManager::new(
        dir.to_path_buf(),
        std::sync::Arc::new(LocalWalletState::new()),
    );
    let id = mgr
        .create_from_slots(
            &req.name,
            NET,
            req.policy,
            req.slots,
            req.encrypt_passphrase.as_deref(),
        )
        .await
        .expect("create_from_slots");
    mgr.unlock_wallet(&id, pass.unwrap_or("").as_bytes())
        .await
        .expect("unlock");
    mgr.peek_address(&id, KeychainKind::External, 0)
        .await
        .expect("address")
}

#[tokio::test]
async fn singlesig_from_library_hot_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = "m/84'/1'/0'";
    let (key, env) = library_key(path, "keychain-pass");
    let slot = WireSlot {
        label: "Laptop key".into(),
        source: SlotSource::from_key_source("generated").unwrap(),
        key,
        secret: Some(open(&env, "keychain-pass")),
    };
    let addr = create(
        dir.path(),
        PolicyDraft::Singlesig,
        vec![slot],
        Some("wallet-pass"),
    )
    .await;
    assert!(addr.starts_with("bcrt1q"), "{addr}");
}

#[tokio::test]
async fn multisig_mixing_hot_and_watch_only_library_keys() {
    let dir = tempfile::tempdir().unwrap();
    let policy = PolicyDraft::Multisig { threshold: 2, n: 3 };
    let layout = layout_for_policy(&policy, "regtest");
    let path = layout.slots[0].derivation_path.clone();
    let mut wire = Vec::new();
    for (i, src) in ["seed", "hardware", "xpub"].into_iter().enumerate() {
        let (key, env) = library_key(&path, "kc");
        let hot = i == 0;
        wire.push(WireSlot {
            label: format!("Key {i}"),
            source: SlotSource::from_key_source(src).unwrap(),
            key,
            secret: hot.then(|| open(&env, "kc")),
        });
    }
    let addr = create(dir.path(), policy, wire, Some("wallet-pass")).await;
    assert!(addr.starts_with("bcrt1q"), "{addr}");
}

#[test]
fn wrong_keychain_password_is_rejected() {
    let (_, env) = library_key("m/84'/1'/0'", "right-password");
    assert!(decrypt_seed(&env, b"wrong-password").is_err());
}
