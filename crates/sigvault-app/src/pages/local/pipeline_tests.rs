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

use super::send::{plain_error, Stage};
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

/// Send flow, step by step through the real core commands: build → sign
/// with the key on this computer → the signed transaction is checked and
/// described (txid, fee) *without* being broadcast, and the UI summary the
/// Broadcast step shows agrees with it. Broadcasting is a separate,
/// explicit command (`cmd_local_broadcast_psbt`, not called here: it needs
/// an Electrum server).
#[tokio::test]
async fn send_signs_then_stops_before_broadcast() {
    use std::str::FromStr;

    use bdk_wallet::bitcoin::{
        absolute, hashes::Hash, transaction, Address, Amount, OutPoint, ScriptBuf, Sequence,
        Transaction, TxIn, TxOut, Txid, Witness,
    };
    use sigvault_core::app_mode::AppMode;
    use sigvault_core::local_wallet::commands::{
        cmd_local_build_psbt, cmd_local_finalize_psbt, cmd_local_sign_psbt_software,
        BuildPsbtRequest, FinalizePsbtRequest, PsbtRecipient, SignPsbtSoftwareRequest,
    };
    use sigvault_core::{AppHandle, ApplicationState, State};

    use crate::pages::cloud::psbt_summary::{hrp_for, summarize};
    use crate::ui::TxSummary;

    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let app = AppHandle::new(dir.path().to_path_buf(), tx);
    let st = ApplicationState::new();
    st.set_app_mode(AppMode::Local).await;

    // A single-key wallet with its key on this computer.
    let path = "m/84'/1'/0'";
    let (key, env) = library_key(path, "kc");
    let slot = WireSlot {
        label: "Laptop key".into(),
        source: SlotSource::from_key_source("generated").unwrap(),
        key,
        secret: Some(open(&env, "kc")),
    };
    let policy = PolicyDraft::Singlesig;
    let layout = layout_for_policy(&policy, "regtest");
    let req: CreateFromSlotsRequest = serde_json::from_value(json!({
        "name": "Spending",
        "network": "regtest",
        "policy": to_wire_policy(&policy, &layout),
        "slots": vec![slot],
        "encrypt_passphrase": "wallet-pass",
    }))
    .unwrap();
    let mgr = LocalWalletManager::new(dir.path().to_path_buf(), st.local_wallet_state.clone());
    let id = mgr
        .create_from_slots(&req.name, NET, req.policy, req.slots, Some("wallet-pass"))
        .await
        .unwrap();
    mgr.unlock_wallet(&id, b"wallet-pass").await.unwrap();

    // Fund it with one unconfirmed 100k sat coin.
    let addr = mgr
        .peek_address(&id, KeychainKind::External, 0)
        .await
        .unwrap();
    let spk = Address::from_str(&addr)
        .unwrap()
        .assume_checked()
        .script_pubkey();
    let funding = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([3; 32]), 1),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: spk,
        }],
    };
    {
        let handle = st.local_wallet_state.get(&id).await.unwrap();
        let mut guard = handle.lock().await;
        guard.wallet.apply_unconfirmed_txs([(funding, 1)]);
    }

    // Details → Review: build.
    let dest = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
    let built = cmd_local_build_psbt(
        app.clone(),
        State::new(&st),
        BuildPsbtRequest {
            wallet_id: id.to_string(),
            recipients: vec![PsbtRecipient {
                address: dest.into(),
                amount_sat: 40_000,
            }],
            fee_rate_sat_vb: 2.0,
            policy_path: None,
        },
    )
    .await
    .unwrap();
    let finalize = |psbt: String| {
        let st = &st;
        async move {
            cmd_local_finalize_psbt(State::new(st), FinalizePsbtRequest { psbt_base64: psbt })
                .await
                .unwrap()
        }
    };
    // The unsigned PSBT can't go out.
    let unsigned = finalize(built.psbt_base64.clone()).await;
    assert!(!unsigned.complete);

    // The review summary: recipient + change, absolute fee, total.
    let review = summarize(&built.psbt_base64, hrp_for(Some("regtest"), None)).unwrap();
    let view = TxSummary::from_psbt(&review);
    assert_eq!(view.sending(), 40_000);
    let recipient: Vec<_> = view.outputs.iter().filter(|o| !o.is_change).collect();
    assert_eq!(recipient.len(), 1);
    assert_eq!(recipient[0].address, dest);
    assert_eq!(view.outputs.iter().filter(|o| o.is_change).count(), 1);
    let fee = view.fee_sat.expect("fee known before signing");
    assert_eq!(view.total_leaving(), Some(40_000 + fee));

    // A wrong passphrase fails plainly.
    let err = cmd_local_sign_psbt_software(
        app.clone(),
        State::new(&st),
        SignPsbtSoftwareRequest {
            wallet_id: id.to_string(),
            psbt_base64: built.psbt_base64.clone(),
            passphrase: "nope".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        plain_error(Stage::Sign, &err).headline.as_ref(),
        "That passphrase didn't unlock this wallet's key."
    );

    // Sign: produces the signed transaction, nothing is broadcast.
    let signed = cmd_local_sign_psbt_software(
        app.clone(),
        State::new(&st),
        SignPsbtSoftwareRequest {
            wallet_id: id.to_string(),
            psbt_base64: built.psbt_base64.clone(),
            passphrase: "wallet-pass".into(),
        },
    )
    .await
    .unwrap();
    let ready = finalize(signed.psbt_base64.clone()).await;
    assert!(ready.complete, "{ready:?}");
    assert_eq!(ready.fee_sat, Some(fee));
    assert!(ready.vsize.is_some());

    // The Broadcast step's summary matches the final transaction.
    let fin = summarize(&signed.psbt_base64, hrp_for(Some("regtest"), None)).unwrap();
    assert!(!fin.signed_fingerprints.is_empty());
    let unsigned_txid = {
        use bdk_wallet::bitcoin::Psbt;
        Psbt::from_str(&built.psbt_base64)
            .unwrap()
            .unsigned_tx
            .compute_txid()
            .to_string()
    };
    assert_eq!(ready.txid, Some(unsigned_txid));
    assert_eq!(
        TxSummary::from_psbt(&fin).total_leaving(),
        Some(40_000 + fee)
    );
}
