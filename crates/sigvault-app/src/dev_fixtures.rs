//! DEBUG-ONLY screenshot fixtures for the SigVault Cloud pages, which
//! otherwise need a real account and a live session websocket.
//!
//! `SIGVAULT_DEV_CLOUD_FIXTURE=signed_out|signed_out_login|signed_in|session_register|session_sign`
//!
//! - `signed_out` switches the workspace to SigVault Cloud for real
//!   (`cmd_set_app_mode`), landing on Network / Sign in; `signed_out_login`
//!   also selects the first available network (`cmd_set_environment`).
//!   Both persist to `SIGVAULT_DATA_DIR`, so point that at a scratch copy.
//! - the others fake a signed-in state after startup: the backend's
//!   in-memory app mode + user profile are set (nothing is persisted, no
//!   token is stored), and the [`AppStore`] gets `authenticated`, a few
//!   remote sessions and — for `session_*` — an active, connected session
//!   whose `session_state` is shaped like the coordinator's workflow
//!   messages (see `session_details.rs` / `signing.rs`).
//!
//! `SIGVAULT_DEV_PAGE=/cloud/settings` picks another cloud page afterwards.
//! With `SIGVAULT_E2E_SIGNER_MNEMONIC` set, the core's debug software signer
//! stands in for a hardware wallet; `session_sign` then lists its
//! fingerprint as a signer so device discovery finds an authorized key.
//!
//! The whole module is compiled only with `debug_assertions`.
#![cfg(debug_assertions)]

use base64::Engine;
use gpui::App;
use serde_json::{json, Value};
use sigvault_core::app_mode::AppMode;
use sigvault_core::commands::{
    cmd_discover_hardware_wallets, cmd_list_environments, cmd_set_app_mode, cmd_set_environment,
};
use sigvault_core::hwi::{DeviceState, DiscoveredDevice};
use sigvault_core::window::WindowApplicationRoute;
use sigvault_core::State;

use crate::router::Page;
use crate::runtime::backend;
use crate::state::{store, RemoteSession, SessionState, StoreEvent};

const ENV: &str = "SIGVAULT_DEV_CLOUD_FIXTURE";

/// Call once at startup (after the store exists). No-op unless the env
/// var is set.
pub fn install(cx: &mut App) {
    let Ok(kind) = std::env::var(ENV) else { return };
    log::warn!("{ENV}={kind}: injecting fake cloud state (debug build)");
    let st = store(cx);
    let mut done = false;
    // Wait for `cmd_initialize_app` to route once, then apply.
    cx.subscribe(&st, move |_, ev: &StoreEvent, cx| {
        if done || !matches!(ev, StoreEvent::RouteChanged) {
            return;
        }
        done = true;
        let kind = kind.clone();
        cx.defer(move |cx| apply(&kind, cx));
    })
    .detach();
}

fn apply(kind: &str, cx: &mut App) {
    if kind == "signed_out" {
        crate::shell::switch_space(AppMode::Cloud, cx);
        return;
    }
    if kind == "signed_out_login" {
        // Cloud workspace with the first available network selected.
        let task = backend(cx).call(|app, st| async move {
            cmd_set_app_mode(app.clone(), State::new(&st), AppMode::Cloud).await?;
            let envs = cmd_list_environments(app.clone(), State::new(&st)).await?;
            if let Some(env) = envs.environments.into_iter().find(|e| !e.coming_soon) {
                cmd_set_environment(app, State::new(&st), env.id).await?;
            }
            Ok(())
        });
        cx.spawn(async move |_| {
            if let Err(e) = task.await {
                log::error!("{ENV}: {e}");
            }
        })
        .detach();
        return;
    }
    let session = match kind {
        "session_register" => Some("DEVICE_REGISTRATION"),
        "session_sign" => Some("TRANSACTION_SIGNING"),
        _ => None,
    };
    let task = backend(cx).call(|app, st| async move {
        st.set_app_mode(AppMode::Cloud).await;
        {
            let mut u = st.user_data.lock().await;
            u.id = Some("dev-user".into());
            u.email = Some("alex@northwind.example".into());
            u.name = Some("Alex Rivera".into());
            u.username = Some("alex".into());
        }
        // The debug software signer's fingerprint, if active.
        let e2e = std::env::var("SIGVAULT_E2E_SIGNER_MNEMONIC").is_ok();
        let fp = match app.get_webview_window("main").filter(|_| e2e) {
            Some(window) => cmd_discover_hardware_wallets(app.clone(), window, State::new(&st), None)
                .await
                .ok()
                .and_then(|r| r.data)
                .and_then(|d| serde_json::from_value::<Vec<DiscoveredDevice>>(d).ok())
                .and_then(|ds| {
                    ds.into_iter().find_map(|d| match d.state {
                        DeviceState::Supported { fingerprint, .. } => Some(fingerprint),
                        _ => None,
                    })
                }),
            None => None,
        };
        Ok::<_, String>(fp)
    });
    let kind = kind.to_string();
    cx.spawn(async move |cx| {
        let signer_fp = task.await.ok().flatten();
        cx.update(|cx| inject(&kind, session, signer_fp, cx)).ok();
    })
    .detach();
}

fn inject(kind: &str, session: Option<&'static str>, signer_fp: Option<String>, cx: &mut App) {
    let ago = |mins: i64| (chrono::Utc::now() - chrono::Duration::minutes(mins)).to_rfc3339();
    let sessions = vec![
        rs("4f1c9a2e-7b3d-4e8a-9c21-5d6e7f8a9b0c", "ACTIVE", "TRANSACTION_SIGNING", Some(ago(4))),
        rs("a83e51d0-2c47-4b9f-8e13-0f2a6b7c8d9e", "ACTIVE", "DEVICE_REGISTRATION", Some(ago(52))),
        rs("c2d7e9f1-5a3b-4c6d-8e0f-1a2b3c4d5e6f", "COMPLETED", "TRANSACTION_SIGNING", Some(ago(60 * 26))),
        rs("e5f6a7b8-9c0d-4e1f-a2b3-c4d5e6f7a8b9", "EXPIRED", "DEVICE_REGISTRATION", Some(ago(60 * 24 * 9))),
    ];
    let route = if session.is_some() {
        WindowApplicationRoute::SessionDetails
    } else {
        WindowApplicationRoute::RemoteSessions
    };
    let state = session.map(|ty| session_state(ty, signer_fp));
    let session_id = match session {
        Some("DEVICE_REGISTRATION") => "a83e51d0-2c47-4b9f-8e13-0f2a6b7c8d9e",
        _ => "4f1c9a2e-7b3d-4e8a-9c21-5d6e7f8a9b0c",
    };
    store(cx).update(cx, |s, cx| {
        s.app_mode = Some(AppMode::Cloud);
        cx.emit(StoreEvent::ModeChanged);
        s.authenticated = true;
        cx.emit(StoreEvent::AuthChanged);
        s.remote_sessions = sessions;
        if let Some(state) = state {
            s.active_session.is_connected = true;
            s.active_session.session_id = Some(session_id.into());
            s.active_session.session_state = state;
            cx.emit(StoreEvent::SessionUpdated);
        }
        s.route = route;
        cx.emit(StoreEvent::RouteChanged);
        cx.notify();
    });
    let page = std::env::var("SIGVAULT_DEV_PAGE")
        .ok()
        .and_then(|p| Page::from_path(&p))
        .filter(Page::is_cloud)
        .unwrap_or(if session.is_some() { Page::SessionDetails } else { Page::Sessions });
    log::info!("{ENV}={kind}: showing {}", page.path());
    crate::shell::navigate(page, cx);
}

fn rs(id: &str, status: &str, ty: &str, created_at: Option<String>) -> RemoteSession {
    RemoteSession { id: id.into(), status: status.into(), session_type: ty.into(), created_at }
}

/// `session_state` as folded from a coordinator workflow message.
fn session_state(ty: &str, signer_fp: Option<String>) -> SessionState {
    let (requirements, data): (Value, Option<Value>) = match ty {
        "DEVICE_REGISTRATION" => (json!({ "network": "testnet", "derivation_path": "m/48'/1'/0'/2'" }), None),
        _ => (
            json!({ "network": "testnet" }),
            Some(json!({
                "transaction": {
                    "psbt": fixture_psbt(),
                    "txid": "9b2f4c1e7a3d5b8f0e6c2a4d1b9f7e3c5a8d0b2e4f6a1c3e5d7b9f0a2c4e6b81",
                    "wallet_name": "Northwind treasury",
                    "multipath_descriptor": "wsh(sortedmulti(2,[3f8a91c2/48'/1'/0'/2']tpubDEXAMPLEa/<0;1>/*,[b71d04e9/48'/1'/0'/2']tpubDEXAMPLEb/<0;1>/*,[5c2e88a0/48'/1'/0'/2']tpubDEXAMPLEc/<0;1>/*))#fixture0",
                    "ledger_hmacs": null
                },
                "signature_slots": [
                    { "fingerprint": signer_fp.unwrap_or_else(|| "3f8a91c2".into()), "derivation_path": "m/48'/1'/0'/2'" },
                    { "fingerprint": "b71d04e9", "derivation_path": "m/48'/1'/0'/2'" }
                ]
            })),
        ),
    };
    SessionState {
        session_type: Some(ty.into()),
        step: Some(1),
        requirements: Some(requirements),
        data,
        finished: Some(false),
        message: None,
        error: None,
    }
}

// ── a small but well-formed PSBT v0: 2 P2WSH inputs → recipient + change ──

fn varint(out: &mut Vec<u8>, n: usize) {
    match n {
        0..=0xfc => out.push(n as u8),
        _ => {
            out.push(0xfd);
            out.extend((n as u16).to_le_bytes());
        }
    }
}

fn kv(out: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    varint(out, key.len());
    out.extend(key);
    varint(out, value.len());
    out.extend(value);
}

fn p2wsh(seed: u8) -> Vec<u8> {
    let mut s = vec![0x00, 0x20];
    s.extend((0..32u8).map(|i| i.wrapping_mul(37).wrapping_add(seed)));
    s
}

fn txout(value: u64, script: &[u8]) -> Vec<u8> {
    let mut o = value.to_le_bytes().to_vec();
    varint(&mut o, script.len());
    o.extend(script);
    o
}

fn fixture_psbt() -> String {
    let inputs = [(0x11u8, 50_000_000u64), (0x22, 49_500_000)];
    let outputs = [(p2wsh(0x5a), 25_000_000u64, false), (p2wsh(0x7c), 74_490_000, true)];

    let mut tx = 2u32.to_le_bytes().to_vec();
    varint(&mut tx, inputs.len());
    for (seed, _) in &inputs {
        tx.extend([*seed; 32]);
        tx.extend(0u32.to_le_bytes());
        varint(&mut tx, 0);
        tx.extend(0xfffffffdu32.to_le_bytes());
    }
    varint(&mut tx, outputs.len());
    for (script, value, _) in &outputs {
        tx.extend(txout(*value, script));
    }
    tx.extend(0u32.to_le_bytes());

    let mut psbt = b"psbt\xff".to_vec();
    kv(&mut psbt, &[0x00], &tx);
    psbt.push(0x00);
    for (seed, value) in &inputs {
        kv(&mut psbt, &[0x01], &txout(*value, &p2wsh(*seed)));
        psbt.push(0x00);
    }
    // Generator point: a valid compressed pubkey for the change derivation.
    let pk: Vec<u8> = (0..33)
        .map(|i| u8::from_str_radix(&"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    for (_, _, change) in &outputs {
        if *change {
            let mut key = vec![0x02];
            key.extend(&pk);
            let mut val = vec![0x3f, 0x8a, 0x91, 0xc2];
            for c in [48 | 1 << 31, 1 | 1 << 31, 1 << 31, 2 | 1 << 31, 1, 7u32] {
                val.extend(c.to_le_bytes());
            }
            kv(&mut psbt, &key, &val);
        }
        psbt.push(0x00);
    }
    base64::engine::general_purpose::STANDARD.encode(psbt)
}
