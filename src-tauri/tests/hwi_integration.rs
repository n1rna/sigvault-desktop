//! Integration tests for the Hardware Wallet Interface (HWI) module.
//!
//! These tests require real hardware wallets to be connected to the computer.
//! They are marked with `#[ignore]` so they don't run in CI.
//!
//! Run all HWI tests:
//!   cargo test -p sigvault-desktop --test hwi_integration -- --ignored --nocapture
//!
//! Run a specific test:
//!   cargo test -p sigvault-desktop --test hwi_integration test_discover_devices -- --ignored --nocapture
//!
//! Environment variables:
//!   BITCOIN_NETWORK  - Network to use: "regtest" (default), "testnet", "signet", "bitcoin"
//!   TEST_PSBT        - Base64-encoded PSBT to use for signing tests
//!   RUST_LOG         - Log level: "debug", "info" (default), "warn", "error"
//!                      Use "debug" for maximum diagnostics

use bitcoin::bip32::{ChildNumber, DerivationPath, Fingerprint};
use bitcoin::Network;
use hex;
use sigvault_desktop_lib::hwi::{
    DeviceState, DiscoveredDevice, HardwareWalletManager, UnsupportedReason, WalletConfig,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

fn test_network() -> Network {
    match std::env::var("BITCOIN_NETWORK")
        .unwrap_or_else(|_| "regtest".to_string())
        .to_lowercase()
        .as_str()
    {
        "testnet" => Network::Testnet,
        "signet" => Network::Signet,
        "bitcoin" | "mainnet" => Network::Bitcoin,
        _ => Network::Regtest,
    }
}

/// Placeholder PSBT - replace with a real one for your wallet, or set TEST_PSBT env var.
/// This is a minimal valid (but unsigned) PSBT for regtest. You almost certainly need
/// to replace this with a PSBT that matches your device's keys.
const DEFAULT_TEST_PSBT: &str = concat!(
    "cHNidP8BAFUCAAAAASeaIyOl37UfxF8iD6WLD8E+HjNCeSqF1+Ns06Et4AdkAAAAAAD/////",
    "AQBgAAAAAAAAFgAUHEF7hrnrFNEq5MXGXZ8+rFnbSJkAAAAAAAA"
);

fn test_psbt() -> String {
    std::env::var("TEST_PSBT").unwrap_or_else(|_| DEFAULT_TEST_PSBT.to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn init_logging() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("sigvault_desktop_lib", log::LevelFilter::Debug)
        .filter_module("async_hwi", log::LevelFilter::Debug)
        .filter_module("hwi_integration", log::LevelFilter::Debug)
        .is_test(true)
        .try_init();
}

/// Parse account-level key origin paths from a descriptor string.
///
/// Extracts entries like `[7a143832/86'/1'/0']` and returns a map of
/// fingerprint -> account derivation path. This is used to fix up PSBTs
/// whose tap_key_origins only contain the relative child path (e.g. `0/0`)
/// instead of the full path from master (e.g. `86'/1'/0'/0/0`).
fn parse_descriptor_key_origins(descriptor: &str) -> Vec<(Fingerprint, DerivationPath)> {
    use std::str::FromStr;

    let mut origins = Vec::new();
    // Match patterns like [fingerprint/path]
    let mut rest = descriptor;
    while let Some(start) = rest.find('[') {
        let after_bracket = &rest[start + 1..];
        if let Some(end) = after_bracket.find(']') {
            let content = &after_bracket[..end];
            // content is like "7a143832/86'/1'/0'"
            if let Some(slash_pos) = content.find('/') {
                let fp_hex = &content[..slash_pos];
                let path_str = &content[slash_pos..]; // "/86'/1'/0'"
                let full_path_str = format!("m{}", path_str);

                if let (Ok(fp), Ok(path)) = (
                    <Fingerprint as FromStr>::from_str(fp_hex),
                    DerivationPath::from_str(&full_path_str),
                ) {
                    println!(
                        "  Parsed key origin: fp={} account_path={}",
                        fp, path
                    );
                    origins.push((fp, path));
                }
            }
            rest = &after_bracket[end + 1..];
        } else {
            break;
        }
    }
    origins
}

/// Simulate what the BitBox02's Transaction::from_psbt() does and print
/// detailed diagnostics about how each input/output would be classified.
/// This helps debug "invalid input" errors from the firmware.
fn diagnose_psbt_for_bitbox(psbt_b64: &str, device_fingerprint: &str) {
    use bitcoin::base64::{engine::general_purpose::STANDARD, Engine};
    use bitcoin::Psbt;

    println!("\n  === BitBox02 Signing Diagnostics ===");
    println!("  Device fingerprint: {}", device_fingerprint);

    let fp_bytes = match hex::decode(device_fingerprint) {
        Ok(b) => b,
        Err(e) => {
            println!("  ERROR: cannot decode fingerprint: {}", e);
            return;
        }
    };

    let bytes = match STANDARD.decode(psbt_b64) {
        Ok(b) => b,
        Err(e) => {
            println!("  ERROR: cannot decode PSBT base64: {}", e);
            return;
        }
    };
    let psbt = match Psbt::deserialize(&bytes) {
        Ok(p) => p,
        Err(e) => {
            println!("  ERROR: cannot deserialize PSBT: {}", e);
            return;
        }
    };

    // Simulate find_our_key for each input
    println!("\n  --- Input classification (simulating find_our_key) ---");
    for (i, input) in psbt.inputs.iter().enumerate() {
        println!("  Input #{}:", i);
        println!(
            "    tap_internal_key: {}",
            input
                .tap_internal_key
                .map_or("MISSING".to_string(), |k| k.to_string())
        );

        let mut found = false;
        for (xonly, (leaf_hashes, (fp, path))) in &input.tap_key_origins {
            let is_ours = &fp.as_bytes()[..] == fp_bytes.as_slice();
            if !is_ours {
                continue;
            }

            found = true;
            let is_internal = input
                .tap_internal_key
                .map_or(false, |ik| &ik == xonly);
            let leaves = leaf_hashes.len();

            if is_internal && leaves == 0 {
                println!(
                    "    -> OUR KEY (internal): pk={}... path={} -> TaprootInternal",
                    &xonly.to_string()[..16],
                    path
                );
            } else if is_internal && leaves > 0 {
                println!(
                    "    -> OUR KEY (internal BUT has {} leaves): pk={}... path={} -> ERROR KeyNotUnique!",
                    leaves,
                    &xonly.to_string()[..16],
                    path
                );
            } else if leaves == 1 {
                println!(
                    "    -> OUR KEY (script path): pk={}... path={} leaf={} -> TaprootScript",
                    &xonly.to_string()[..16],
                    path,
                    leaf_hashes[0]
                );
            } else {
                println!(
                    "    -> OUR KEY but {} leaves (expected 1): pk={}... path={} -> ERROR KeyNotUnique!",
                    leaves,
                    &xonly.to_string()[..16],
                    path
                );
            }
            break; // find_our_key returns on first match
        }
        if !found {
            println!("    -> OUR KEY NOT FOUND in tap_key_origins -> ERROR KeyNotFound!");
        }
    }

    // Simulate find_our_key for each output
    println!("\n  --- Output classification (simulating find_our_key) ---");
    for (i, (tx_out, output)) in psbt
        .unsigned_tx
        .output
        .iter()
        .zip(psbt.outputs.iter())
        .enumerate()
    {
        println!("  Output #{}: {} sats", i, tx_out.value.to_sat());
        println!(
            "    tap_internal_key: {}",
            output
                .tap_internal_key
                .map_or("MISSING".to_string(), |k| k.to_string())
        );
        println!("    tap_key_origins: {} keys", output.tap_key_origins.len());

        for (xonly, (leaf_hashes, (fp, path))) in &output.tap_key_origins {
            let is_ours = &fp.as_bytes()[..] == fp_bytes.as_slice();
            println!(
                "      pk={}... fp={} path={} leaves={} {}",
                &xonly.to_string()[..16],
                fp,
                path,
                leaf_hashes.len(),
                if is_ours { "<- OURS" } else { "" }
            );
        }

        if output.tap_key_origins.is_empty() {
            println!("    -> EXTERNAL output (no tap_key_origins)");
            continue;
        }

        let mut found = false;
        for (xonly, (leaf_hashes, (fp, path))) in &output.tap_key_origins {
            let is_ours = &fp.as_bytes()[..] == fp_bytes.as_slice();
            if !is_ours {
                continue;
            }
            found = true;
            let is_internal = output
                .tap_internal_key
                .map_or(false, |ik| &ik == xonly);
            let leaves = leaf_hashes.len();

            if is_internal && leaves == 0 {
                println!(
                    "    -> INTERNAL output (our key is tap_internal_key, keypath={})",
                    path
                );
            } else if !is_internal && output.tap_internal_key.is_none() && leaves == 0 {
                println!(
                    "    -> PROBLEM: our key has 0 leaves but tap_internal_key is MISSING!"
                );
                println!("       find_our_key will return KeyNotUnique -> output treated as EXTERNAL");
                println!("       FIX: set tap_internal_key on this output");
            } else if leaves == 1 {
                println!(
                    "    -> INTERNAL output (our key is in script path, keypath={})",
                    path
                );
            } else {
                println!(
                    "    -> PROBLEM: our key has {} leaves, expected 0 (internal) or 1 (script)",
                    leaves
                );
                println!("       find_our_key will return KeyNotUnique -> output treated as EXTERNAL");
            }
            break;
        }
        if !found {
            println!("    -> EXTERNAL output (our fingerprint not in tap_key_origins)");
        }
    }

    // Summary
    println!("\n  --- Transaction summary for firmware ---");
    println!("    version:  {}", psbt.unsigned_tx.version);
    println!("    locktime: {}", psbt.unsigned_tx.lock_time);
    println!("    inputs:   {}", psbt.inputs.len());
    println!("    outputs:  {}", psbt.unsigned_tx.output.len());
    println!();
}

fn print_separator(title: &str) {
    println!("\n{}", "=".repeat(72));
    println!("  {}", title);
    println!("{}\n", "=".repeat(72));
}

fn print_device_details(device: &DiscoveredDevice) {
    println!("  Device ID:    {}", device.id);
    println!("  Type:         {}", device.device_type);
    println!("  Model:        {}", device.model);
    match &device.state {
        DeviceState::Supported {
            fingerprint,
            version,
            registered,
        } => {
            println!("  State:        SUPPORTED");
            println!("  Fingerprint:  {}", fingerprint);
            println!(
                "  Version:      {}",
                version.as_deref().unwrap_or("unknown")
            );
            println!(
                "  Registered:   {}",
                match registered {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "n/a",
                }
            );
        }
        DeviceState::Locked { pairing_code } => {
            println!("  State:        LOCKED");
            if let Some(code) = pairing_code {
                println!("  Pairing Code: {}", code);
            } else {
                println!("  Pairing Code: (none - device needs PIN/auth)");
            }
        }
        DeviceState::Unsupported { reason, version } => {
            println!("  State:        UNSUPPORTED");
            println!(
                "  Version:      {}",
                version.as_deref().unwrap_or("unknown")
            );
            match reason {
                UnsupportedReason::Version {
                    minimal_supported_version,
                } => {
                    println!(
                        "  Reason:       Firmware too old (minimum: {})",
                        minimal_supported_version
                    );
                }
                UnsupportedReason::Method(method) => {
                    println!("  Reason:       Unsupported method: {}", method);
                }
                UnsupportedReason::NotPartOfWallet { fingerprint } => {
                    println!(
                        "  Reason:       Not part of wallet (fingerprint: {})",
                        fingerprint
                    );
                }
                UnsupportedReason::WrongNetwork => {
                    println!("  Reason:       Device configured for a different network");
                }
                UnsupportedReason::AppNotOpen => {
                    println!(
                        "  Reason:       Bitcoin app not open (Ledger - open the Bitcoin app)"
                    );
                }
                UnsupportedReason::InitializationError(msg) => {
                    println!("  Reason:       Initialization error: {}", msg);
                }
            }
        }
    }
    println!();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Discover all connected hardware wallets and print detailed diagnostics.
///
/// This is the main debugging test. It will show you exactly what devices
/// are found, their state, and why any might be unsupported.
#[tokio::test]
#[ignore]
async fn test_discover_devices() {
    init_logging();
    let network = test_network();

    print_separator(&format!("DEVICE DISCOVERY (network: {:?})", network));

    let manager = HardwareWalletManager::new(network);

    let start = Instant::now();
    let result = manager.discover_devices(None, None).await;
    let elapsed = start.elapsed();

    println!("Discovery took: {:.2?}\n", elapsed);

    match result {
        Ok(devices) => {
            if devices.is_empty() {
                println!("  NO DEVICES FOUND");
                println!();
                println!("  Troubleshooting:");
                println!("  - Make sure your hardware wallet is connected via USB");
                println!("  - For Ledger: open the Bitcoin app on the device");
                println!("  - For BitBox02: the device should be powered on");
                println!("  - For Jade: connect via USB (serial port)");
                println!("  - For Coldcard: connect via USB");
                println!("  - Check USB permissions (Linux: udev rules)");
                println!("  - Try running with RUST_LOG=debug for more details");
            } else {
                println!("  Found {} device(s):\n", devices.len());
                for (i, device) in devices.iter().enumerate() {
                    println!("  --- Device #{} ---", i + 1);
                    print_device_details(device);
                }
            }

            // Summary
            let supported = devices.iter().filter(|d| d.is_supported()).count();
            let locked = devices.iter().filter(|d| d.is_locked()).count();
            let unsupported = devices.len() - supported - locked;

            print_separator("SUMMARY");
            println!("  Total:       {}", devices.len());
            println!("  Supported:   {}", supported);
            println!("  Locked:      {}", locked);
            println!("  Unsupported: {}", unsupported);
        }
        Err(e) => {
            println!("  DISCOVERY FAILED: {:?}", e);
            panic!("Device discovery failed: {:?}", e);
        }
    }
}

/// Discover devices with a wallet configuration (descriptor).
///
/// This tests whether devices can register/recognize a wallet policy.
/// Set the descriptor via environment variables or edit the constants below.
#[tokio::test]
#[ignore]
async fn test_discover_with_wallet_config() {
    init_logging();
    let network = test_network();

    print_separator(&format!(
        "DEVICE DISCOVERY WITH WALLET CONFIG (network: {:?})",
        network
    ));

    // Edit these to match your wallet configuration
    let wallet_name =
        std::env::var("TEST_WALLET_NAME").unwrap_or_else(|_| "test-wallet".to_string());
    let descriptor = std::env::var("TEST_DESCRIPTOR").ok();

    let wallet_config = WalletConfig {
        name: wallet_name.clone(),
        descriptor: descriptor.clone(),
        hmac: None,
    };

    println!("  Wallet name:  {}", wallet_name);
    println!(
        "  Descriptor:   {}",
        descriptor
            .as_deref()
            .unwrap_or("(none - set TEST_DESCRIPTOR)")
    );
    println!();

    let manager = HardwareWalletManager::new(network);

    let start = Instant::now();
    let result = manager.discover_devices(Some(&wallet_config), None).await;
    let elapsed = start.elapsed();

    println!("Discovery took: {:.2?}\n", elapsed);

    match result {
        Ok(devices) => {
            println!("  Found {} device(s):\n", devices.len());
            for (i, device) in devices.iter().enumerate() {
                println!("  --- Device #{} ---", i + 1);
                print_device_details(device);
            }
        }
        Err(e) => {
            println!("  DISCOVERY FAILED: {:?}", e);
            panic!("Device discovery with wallet config failed: {:?}", e);
        }
    }
}

/// Discover devices, unlock any that are locked, then report final state.
///
/// WARNING: This test requires physical interaction with locked devices:
/// - BitBox02: confirm pairing on device screen
/// - Jade: enter PIN on device
///
/// The test will wait for you to interact with the device.
#[tokio::test]
#[ignore]
async fn test_discover_and_unlock() {
    init_logging();
    let network = test_network();

    // Build wallet config from env vars (same as discover_with_wallet_config)
    let wallet_name =
        std::env::var("TEST_WALLET_NAME").unwrap_or_else(|_| "test-wallet".to_string());
    let descriptor = std::env::var("TEST_DESCRIPTOR").ok();

    let wallet_config = WalletConfig {
        name: wallet_name.clone(),
        descriptor: descriptor.clone(),
        hmac: None,
    };

    print_separator(&format!("DISCOVER AND UNLOCK (network: {:?})", network));
    println!("  Wallet name:  {}", wallet_name);
    println!(
        "  Descriptor:   {}",
        descriptor
            .as_deref()
            .unwrap_or("(none - set TEST_DESCRIPTOR)")
    );
    println!();

    let manager = HardwareWalletManager::new(network);

    // Step 1: Discover with wallet config
    println!("  Step 1: Discovering devices...");
    let devices = manager
        .discover_devices(Some(&wallet_config), None)
        .await
        .expect("Discovery failed");

    println!("  Found {} device(s)\n", devices.len());
    for device in &devices {
        print_device_details(device);
    }

    // Step 2: Unlock locked devices (passing wallet config for policy registration)
    let locked: Vec<_> = devices.iter().filter(|d| d.is_locked()).collect();

    if locked.is_empty() {
        println!("  No locked devices to unlock.");
        return;
    }

    for device in &locked {
        print_separator(&format!(
            "UNLOCKING: {} ({})",
            device.id, device.device_type
        ));

        if let DeviceState::Locked { pairing_code } = &device.state {
            if let Some(code) = pairing_code {
                println!("  Pairing code: {}", code);
                println!("  >>> Please confirm on your device <<<\n");
            } else {
                println!("  >>> Please enter PIN / authenticate on your device <<<\n");
            }
        }

        let start = Instant::now();
        match manager
            .unlock_device(&device.id, Some(&wallet_config), None)
            .await
        {
            Ok(unlocked) => {
                println!("  Unlocked in {:.2?}\n", start.elapsed());
                print_device_details(&unlocked);
            }
            Err(e) => {
                println!("  UNLOCK FAILED after {:.2?}", start.elapsed());
                println!("  Error: {:?}\n", e);
            }
        }
    }

    // Step 3: Report final supported devices
    print_separator("FINAL STATE - ALL SUPPORTED DEVICES");
    let supported: Vec<_> = devices.iter().filter(|d| d.is_supported()).collect();
    let newly_unlocked_count = locked.len();
    println!("  Originally supported: {}", supported.len());
    println!("  Attempted unlocks:    {}", newly_unlocked_count);
}

/// Extract xpub from each supported device at a standard derivation path.
///
/// Tests the get_device_info flow for each discovered device.
#[tokio::test]
#[ignore]
async fn test_get_device_xpubs() {
    init_logging();
    let network = test_network();

    let derivation_paths: Vec<&str> = match network {
        Network::Bitcoin => vec![
            "m/84'/0'/0'", "m/86'/0'/0'", "m/48'/0'/0'/2'",
            "m/84'/1'/0'", "m/86'/1'/0'", "m/48'/1'/0'/2'",
        ],
        _ => vec![
            "m/84'/1'/0'", "m/86'/1'/0'", "m/48'/1'/0'/2'",
            "m/84'/0'/0'", "m/86'/0'/0'", "m/48'/0'/0'/2'",
        ],
    };

    print_separator(&format!(
        "EXTRACT XPUBS (network: {:?}, paths: {:?})",
        network, derivation_paths
    ));

    let manager = HardwareWalletManager::new(network);

    // Discover
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let supported: Vec<_> = devices.iter().filter(|d| d.is_supported()).collect();

    if supported.is_empty() {
        println!("  No supported devices found. Cannot extract xpubs.");
        println!("  Plug in a device and make sure it's unlocked.");
        return;
    }

    for device in &supported {
        let fingerprint = device.fingerprint().unwrap();
        println!(
            "  --- {} ({}) [{}] ---",
            device.device_type, device.id, fingerprint
        );

        for derivation_path in &derivation_paths {
            let start = Instant::now();
            match manager.get_device_info(fingerprint, derivation_path).await {
                Ok(info) => {
                    println!("  [{}] Extracted in {:.2?}", derivation_path, start.elapsed());
                    println!("    Xpub:        {}", info.xpub);
                    println!("    Fingerprint: {}", info.fingerprint);
                    println!("    Device Type: {}", info.device_type);
                }
                Err(e) => {
                    println!("  [{}] FAILED after {:.2?}: {:?}", derivation_path, start.elapsed(), e);
                }
            }
        }
        println!();
    }
}

/// Sign a PSBT with each supported device.
///
/// Set the TEST_PSBT environment variable to a base64-encoded PSBT that
/// matches your device's keys, or edit DEFAULT_TEST_PSBT above.
///
/// WARNING: This will prompt for signing confirmation on the device.
#[tokio::test]
#[ignore]
async fn test_sign_psbt() {
    init_logging();
    let network = test_network();
    let psbt_b64 = test_psbt();

    print_separator(&format!("SIGN PSBT (network: {:?})", network));

    println!(
        "  PSBT (first 80 chars): {}...",
        &psbt_b64[..80.min(psbt_b64.len())]
    );
    println!(
        "  PSBT length:           {} bytes (base64)\n",
        psbt_b64.len()
    );

    // Validate and dump full PSBT details
    {
        use bitcoin::base64::{engine::general_purpose::STANDARD, Engine};
        use bitcoin::Psbt;

        match STANDARD.decode(&psbt_b64) {
            Ok(bytes) => match Psbt::deserialize(&bytes) {
                Ok(psbt) => {
                    println!("  PSBT parsed successfully:");
                    println!("  Inputs:  {}", psbt.inputs.len());
                    println!("  Outputs: {}", psbt.outputs.len());
                    println!(
                        "  TX locktime: {}",
                        psbt.unsigned_tx.lock_time
                    );
                    println!(
                        "  TX version:  {}",
                        psbt.unsigned_tx.version
                    );

                    for (i, (tx_in, input)) in psbt
                        .unsigned_tx
                        .input
                        .iter()
                        .zip(psbt.inputs.iter())
                        .enumerate()
                    {
                        println!("\n  === Input #{} ===", i);
                        println!(
                            "    prevout:    {}:{}",
                            tx_in.previous_output.txid, tx_in.previous_output.vout
                        );
                        println!("    sequence:   {}", tx_in.sequence);

                        if let Some(ref utxo) = input.witness_utxo {
                            println!("    witness_utxo:");
                            println!("      amount:   {} sats", utxo.value.to_sat());
                            println!("      script:   {}", utxo.script_pubkey.to_hex_string());
                        } else {
                            println!("    witness_utxo: MISSING");
                        }

                        if let Some(ref ik) = input.tap_internal_key {
                            println!("    tap_internal_key: {}", ik);
                        } else {
                            println!("    tap_internal_key: MISSING");
                        }

                        if let Some(ref mr) = input.tap_merkle_root {
                            println!("    tap_merkle_root:  {}", mr);
                        }

                        if let Some(sighash) = input.sighash_type {
                            println!("    sighash_type:     {:?}", sighash);
                        } else {
                            println!("    sighash_type:     (default)");
                        }

                        if !input.tap_key_origins.is_empty() {
                            println!("    tap_key_origins:");
                            for (pubkey, (leaf_hashes, (fp, path))) in &input.tap_key_origins {
                                println!(
                                    "      pk: {} fp: {} path: {} leaves: {}",
                                    pubkey, fp, path, leaf_hashes.len()
                                );
                                for lh in leaf_hashes {
                                    println!("        leaf_hash: {}", lh);
                                }
                            }
                        }

                        if !input.tap_scripts.is_empty() {
                            println!("    tap_scripts ({}):", input.tap_scripts.len());
                            for (control_block, (script, leaf_ver)) in &input.tap_scripts {
                                println!(
                                    "      leaf_ver: {:?} script: {} control: {} bytes",
                                    leaf_ver,
                                    script.to_hex_string(),
                                    control_block.serialize().len()
                                );
                            }
                        }
                    }

                    for (i, (tx_out, output)) in psbt
                        .unsigned_tx
                        .output
                        .iter()
                        .zip(psbt.outputs.iter())
                        .enumerate()
                    {
                        println!("\n  === Output #{} ===", i);
                        println!("    amount: {} sats", tx_out.value.to_sat());
                        println!("    script: {}", tx_out.script_pubkey.to_hex_string());

                        if let Some(ref ik) = output.tap_internal_key {
                            println!("    tap_internal_key: {}", ik);
                        }

                        if !output.tap_key_origins.is_empty() {
                            println!("    tap_key_origins:");
                            for (pubkey, (leaf_hashes, (fp, path))) in &output.tap_key_origins {
                                println!(
                                    "      pk: {} fp: {} path: {} leaves: {}",
                                    pubkey, fp, path, leaf_hashes.len()
                                );
                            }
                        }
                    }
                    println!();
                }
                Err(e) => {
                    println!("  WARNING: PSBT deserialization failed: {:?}", e);
                    println!("  The signing test will likely fail.\n");
                }
            },
            Err(e) => {
                println!("  WARNING: Base64 decode failed: {:?}", e);
                println!("  Set TEST_PSBT env var to a valid base64 PSBT.\n");
            }
        }
    }

    // Build wallet config from env vars
    let wallet_name =
        std::env::var("TEST_WALLET_NAME").unwrap_or_else(|_| "test-wallet".to_string());
    let descriptor = std::env::var("TEST_DESCRIPTOR").ok();

    let wallet_config = WalletConfig {
        name: wallet_name.clone(),
        descriptor: descriptor.clone(),
        hmac: None,
    };

    println!("  Wallet name:  {}", wallet_name);
    println!(
        "  Descriptor:   {}",
        descriptor
            .as_deref()
            .unwrap_or("(none - set TEST_DESCRIPTOR)")
    );
    println!();

    let manager = HardwareWalletManager::new(network);

    // Step 1: Discover with wallet config
    println!("  Step 1: Discovering devices...");
    let devices = manager
        .discover_devices(Some(&wallet_config), None)
        .await
        .expect("Discovery failed");

    println!("  Found {} device(s)\n", devices.len());
    for device in &devices {
        print_device_details(device);
    }

    // Step 2: Unlock any locked devices
    let locked: Vec<_> = devices.iter().filter(|d| d.is_locked()).collect();

    if !locked.is_empty() {
        println!("  Step 2: Unlocking {} locked device(s)...\n", locked.len());

        for device in &locked {
            if let DeviceState::Locked { pairing_code } = &device.state {
                if let Some(code) = pairing_code {
                    println!("  Pairing code: {}", code);
                    println!("  >>> Please confirm on your device <<<\n");
                } else {
                    println!("  >>> Please enter PIN / authenticate on your device <<<\n");
                }
            }

            let start = Instant::now();
            match manager
                .unlock_device(&device.id, Some(&wallet_config), None)
                .await
            {
                Ok(unlocked) => {
                    println!("  Unlocked in {:.2?}", start.elapsed());
                    print_device_details(&unlocked);

                    // Verify fingerprint matches descriptor keys
                    if let Some(fp) = unlocked.fingerprint() {
                        if let Some(ref desc) = descriptor {
                            let origins = parse_descriptor_key_origins(desc);
                            let fp_matches: Vec<_> = origins.iter()
                                .filter(|(ofp, _)| ofp.to_string() == fp)
                                .collect();
                            if fp_matches.is_empty() {
                                println!("  WARNING: Device fingerprint {} does NOT match any key in descriptor!", fp);
                                println!("  Descriptor key fingerprints: {:?}", origins.iter().map(|(f,_)| f.to_string()).collect::<Vec<_>>());
                                println!("  Signing will likely fail with 'invalid input'.\n");
                            } else {
                                println!("  Device fingerprint {} matches descriptor key (OK)\n", fp);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("  UNLOCK FAILED after {:.2?}: {:?}", start.elapsed(), e);
                    println!("  This often means the device's keys don't match the descriptor.");
                    println!("  Check that TEST_DESCRIPTOR was generated from THIS specific device.\n");
                }
            }
        }
    } else {
        println!("  Step 2: No locked devices (skipped)\n");
    }

    // Step 3: Sign with all supported devices (original + newly unlocked)
    // Collect device IDs that are now available for signing
    let mut sign_targets: Vec<(String, String, String)> = devices
        .iter()
        .filter(|d| d.is_supported())
        .map(|d| {
            (
                d.id.clone(),
                d.device_type.clone(),
                d.fingerprint().unwrap_or("?").to_string(),
            )
        })
        .collect();

    // Add unlocked devices (they're now in supported_devices on the manager)
    for device in &locked {
        // Check if the manager now has this device as supported
        if manager.get_device(& device.id).await.is_some() {
            sign_targets.push((
                device.id.clone(),
                device.device_type.clone(),
                "unlocked".to_string(),
            ));
        }
    }

    if sign_targets.is_empty() {
        println!("  No supported devices available for signing.");
        return;
    }

    println!("  Step 3: Signing PSBT with {} device(s)...\n", sign_targets.len());

    for (id, dtype, fp) in &sign_targets {
        print_separator(&format!("SIGNING WITH: {} ({}) [{}]", dtype, id, fp));

        // Run pre-signing diagnostics
        if let Some(device) = manager.get_device(id).await {
            // Print firmware version
            match device.get_version().await {
                Ok(v) => println!("  Firmware version: {}", v),
                Err(e) => println!("  Firmware version: unknown ({:?})", e),
            }
            // Get real fingerprint for diagnostics
            if let Ok(real_fp) = device.get_master_fingerprint().await {
                println!("  Device fingerprint: {}", real_fp);
                if dtype.contains("BitBox") {
                    diagnose_psbt_for_bitbox(&psbt_b64, &real_fp.to_string());
                }
            }
        }

        println!("  >>> Please confirm signing on your device <<<\n");

        let start = Instant::now();
        match manager.sign_psbt(id, &psbt_b64).await {
            Ok(signed) => {
                println!("  SIGNED successfully in {:.2?}", start.elapsed());
                println!("  Fingerprint:         {}", signed.fingerprint);
                println!(
                    "  Signed PSBT length:  {} bytes (base64)",
                    signed.psbt.len()
                );
                println!(
                    "  Signed PSBT (first 80): {}...",
                    &signed.psbt[..80.min(signed.psbt.len())]
                );
            }
            Err(e) => {
                println!("  SIGNING FAILED after {:.2?}", start.elapsed());
                println!("  Error: {:?}", e);
                println!("  Error (display): {}", e);
                println!();
                println!("  NOTE: Check stderr (eprintln) output above for step-level");
                println!("  debug logging from btc_sign showing which protocol step failed:");
                println!("  - BtcSignInit: policy parsing/registration lookup failed");
                println!("  - BtcSignInput[N]: input validation failed (keypath, value, etc)");
                println!("  - BtcSignOutput[N]: output validation failed (keypath, value, etc)");
                println!();
                println!("  Troubleshooting:");
                println!("  - Does the PSBT contain inputs matching this device's keys?");
                println!("  - Is the PSBT for the correct network ({:?})?", network);
                println!("  - Did you confirm/approve on the device?");
                println!("  - For BitBox02: is firmware >= v9.21.0 (required for Taproot policies)?");
                println!(
                    "  - For Ledger: is the wallet policy registered?"
                );
            }
        }
        println!();
    }
}

/// Full end-to-end flow: discover -> unlock -> extract xpub -> sign PSBT.
///
/// This is the most comprehensive test. It exercises the entire HWI pipeline.
#[tokio::test]
#[ignore]
async fn test_full_flow() {
    init_logging();
    let network = test_network();
    let psbt_b64 = test_psbt();

    let derivation_path = match network {
        Network::Bitcoin => "m/84'/0'/0'",
        _ => "m/84'/1'/0'",
    };

    print_separator(&format!("FULL E2E FLOW (network: {:?})", network));

    let manager = HardwareWalletManager::new(network);

    // -----------------------------------------------------------------------
    // Phase 1: Discovery
    // -----------------------------------------------------------------------
    println!("  PHASE 1: Discovery\n");

    let start = Instant::now();
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");
    println!(
        "  Discovered {} device(s) in {:.2?}\n",
        devices.len(),
        start.elapsed()
    );

    for device in &devices {
        print_device_details(device);
    }

    if devices.is_empty() {
        println!("  No devices found. Aborting.");
        return;
    }

    // -----------------------------------------------------------------------
    // Phase 2: Unlock
    // -----------------------------------------------------------------------
    let locked_ids: Vec<String> = devices
        .iter()
        .filter(|d| d.is_locked())
        .map(|d| d.id.clone())
        .collect();

    if !locked_ids.is_empty() {
        println!("\n  PHASE 2: Unlocking {} device(s)\n", locked_ids.len());

        for id in &locked_ids {
            println!("  Unlocking {}...", id);
            println!("  >>> Interact with your device to unlock <<<\n");

            match manager.unlock_device(id, None, None).await {
                Ok(unlocked) => {
                    println!("  Unlocked successfully:");
                    print_device_details(&unlocked);
                }
                Err(e) => {
                    println!("  Unlock FAILED: {:?}\n", e);
                }
            }
        }
    } else {
        println!("\n  PHASE 2: No locked devices (skipped)\n");
    }

    // -----------------------------------------------------------------------
    // Phase 3: Extract xpubs
    // -----------------------------------------------------------------------
    // Re-gather supported device fingerprints (includes newly unlocked)
    // We need to re-discover or check the manager's state
    // Since unlock adds to supported_devices, we can query by fingerprint

    // Collect fingerprints from originally supported devices
    let fingerprints: Vec<(String, String, String)> = devices
        .iter()
        .filter(|d| d.is_supported())
        .filter_map(|d| {
            d.fingerprint()
                .map(|fp| (fp.to_string(), d.id.clone(), d.device_type.clone()))
        })
        .collect();

    // Also try to get xpubs from unlocked devices (they're now in supported_devices)
    // We need their fingerprints - get_device will work for them
    println!("  PHASE 3: Extracting xpubs (path: {})\n", derivation_path);

    if fingerprints.is_empty() {
        println!("  No supported devices. Skipping xpub extraction.");
    } else {
        for (fp, id, dtype) in &fingerprints {
            println!("  {} ({}) [{}]:", dtype, id, fp);
            match manager.get_device_info(fp, derivation_path).await {
                Ok(info) => {
                    println!("    xpub: {}", info.xpub);
                }
                Err(e) => {
                    println!("    ERROR: {:?}", e);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Phase 4: Sign PSBT
    // -----------------------------------------------------------------------
    println!("\n  PHASE 4: Signing PSBT\n");
    println!(
        "  PSBT: {}... ({} bytes)",
        &psbt_b64[..40.min(psbt_b64.len())],
        psbt_b64.len()
    );

    // Try signing with each supported device by ID
    let supported_ids: Vec<(String, String)> = devices
        .iter()
        .filter(|d| d.is_supported())
        .map(|d| (d.id.clone(), d.device_type.clone()))
        .collect();

    if supported_ids.is_empty() {
        println!("  No supported devices to sign with.");
    } else {
        for (id, dtype) in &supported_ids {
            println!("\n  Signing with {} ({})...", dtype, id);
            println!("  >>> Confirm on device <<<");

            let start = Instant::now();
            match manager.sign_psbt(id, &psbt_b64).await {
                Ok(signed) => {
                    println!(
                        "  OK in {:.2?} - signed by {}",
                        start.elapsed(),
                        signed.fingerprint
                    );
                }
                Err(e) => {
                    println!("  FAILED in {:.2?}: {:?}", start.elapsed(), e);
                }
            }
        }
    }

    print_separator("DONE");
}

/// Specifically test Ledger device discovery and diagnostics.
///
/// Ledger devices have specific requirements (Bitcoin app must be open)
/// and this test helps debug Ledger-specific issues.
#[tokio::test]
#[ignore]
async fn test_ledger_diagnostics() {
    init_logging();
    let network = test_network();

    print_separator(&format!("LEDGER DIAGNOSTICS (network: {:?})", network));

    println!("  Prerequisites:");
    println!("  1. Ledger device connected via USB");
    println!("  2. Device unlocked (PIN entered)");
    println!("  3. Bitcoin app OPEN on the Ledger");
    println!();

    // Check for Ledger simulator first
    println!("  Checking for Ledger simulator (port 9999)...");
    {
        use async_hwi::ledger::LedgerSimulator;
        match LedgerSimulator::try_connect().await {
            Ok(_) => println!("  Ledger simulator: FOUND"),
            Err(e) => println!("  Ledger simulator: not available ({:?})", e),
        }
    }
    println!();

    // Full discovery
    let manager = HardwareWalletManager::new(network);
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let ledgers: Vec<_> = devices
        .iter()
        .filter(|d| d.device_type == "Ledger")
        .collect();

    if ledgers.is_empty() {
        println!("  No Ledger devices found.");
        println!();
        println!("  Troubleshooting:");
        println!("  - Is the device connected and unlocked?");
        println!("  - Is the Bitcoin app open?");
        println!("  - On Linux: check udev rules for Ledger");
        println!("    sudo cp /path/to/ledger-udev-rules /etc/udev/rules.d/");
        println!("    sudo udevadm control --reload-rules");
        println!("  - Try: lsusb | grep -i ledger");
    } else {
        for device in &ledgers {
            print_device_details(device);
        }
    }
}

/// Specifically test BitBox02 device discovery and pairing.
#[tokio::test]
#[ignore]
async fn test_bitbox02_diagnostics() {
    init_logging();
    let network = test_network();

    print_separator(&format!("BITBOX02 DIAGNOSTICS (network: {:?})", network));

    println!("  Prerequisites:");
    println!("  1. BitBox02 connected via USB");
    println!("  2. Device powered on");
    println!();

    let manager = HardwareWalletManager::new(network);
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let bitboxes: Vec<_> = devices
        .iter()
        .filter(|d| d.device_type == "BitBox02")
        .collect();

    if bitboxes.is_empty() {
        println!("  No BitBox02 devices found.");
        println!();
        println!("  Troubleshooting:");
        println!("  - Is the device connected and powered on?");
        println!("  - On Linux: check udev rules for BitBox02");
        println!("  - Try: lsusb | grep -i shiftcrypto");
    } else {
        for device in &bitboxes {
            print_device_details(device);

            if device.is_locked() {
                println!("  Attempting unlock (confirm pairing on device)...");
                match manager.unlock_device(&device.id, None, None).await {
                    Ok(unlocked) => {
                        println!("  Unlocked:");
                        print_device_details(&unlocked);
                    }
                    Err(e) => {
                        println!("  Unlock FAILED: {:?}", e);
                    }
                }
            }
        }
    }
}

/// Specifically test Coldcard device discovery.
#[tokio::test]
#[ignore]
async fn test_coldcard_diagnostics() {
    init_logging();
    let network = test_network();

    print_separator(&format!("COLDCARD DIAGNOSTICS (network: {:?})", network));

    println!("  Prerequisites:");
    println!("  1. Coldcard connected via USB");
    println!("  2. Device unlocked (PIN entered)");
    println!("  Minimum firmware: 6.2.1 (Edge firmware)");
    println!();

    let manager = HardwareWalletManager::new(network);
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let coldcards: Vec<_> = devices
        .iter()
        .filter(|d| d.device_type == "Coldcard")
        .collect();

    if coldcards.is_empty() {
        println!("  No Coldcard devices found.");
        println!();
        println!("  Troubleshooting:");
        println!("  - Is the device connected via USB and unlocked?");
        println!("  - On Linux: check udev rules for Coinkite devices");
        println!("  - Coldcard VID: 0xd13e, PID: 0xcc10");
        println!("  - Try: lsusb | grep -i coinkite");
    } else {
        for device in &coldcards {
            print_device_details(device);
        }
    }
}

/// Specifically test Jade device discovery.
#[tokio::test]
#[ignore]
async fn test_jade_diagnostics() {
    init_logging();
    let network = test_network();

    print_separator(&format!("JADE DIAGNOSTICS (network: {:?})", network));

    println!("  Prerequisites:");
    println!("  1. Jade connected via USB");
    println!("  2. Device powered on");
    println!();

    // Check serial ports
    println!("  Checking serial ports...");
    match async_hwi::jade::SerialTransport::enumerate_potential_ports() {
        Ok(ports) => {
            if ports.is_empty() {
                println!("  No potential Jade serial ports found.");
            } else {
                for port in &ports {
                    println!("    Port: {}", port);
                }
            }
        }
        Err(e) => println!("  Error enumerating ports: {:?}", e),
    }
    println!();

    let manager = HardwareWalletManager::new(network);
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let jades: Vec<_> = devices.iter().filter(|d| d.device_type == "Jade").collect();

    if jades.is_empty() {
        println!("  No Jade devices found.");
        println!();
        println!("  Troubleshooting:");
        println!("  - Is the device connected via USB?");
        println!("  - On Linux: check that your user is in the 'dialout' group");
        println!("    sudo usermod -a -G dialout $USER");
        println!("  - Check: ls -la /dev/ttyUSB* /dev/ttyACM*");
    } else {
        for device in &jades {
            print_device_details(device);

            if device.is_locked() {
                println!("  Attempting unlock (authenticate on device)...");
                match manager.unlock_device(&device.id, None, None).await {
                    Ok(unlocked) => {
                        println!("  Unlocked:");
                        print_device_details(&unlocked);
                    }
                    Err(e) => {
                        println!("  Unlock FAILED: {:?}", e);
                    }
                }
            }
        }
    }
}

/// Specifically test Trezor device discovery.
#[tokio::test]
#[ignore]
async fn test_trezor_diagnostics() {
    init_logging();
    let network = test_network();

    print_separator(&format!("TREZOR DIAGNOSTICS (network: {:?})", network));

    println!("  Prerequisites:");
    println!("  1. Trezor connected via USB");
    println!("  2. Device unlocked (PIN entered)");
    println!();

    let manager = HardwareWalletManager::new(network);
    let devices = manager
        .discover_devices(None, None)
        .await
        .expect("Discovery failed");

    let trezors: Vec<_> = devices.iter().filter(|d| d.device_type == "Trezor").collect();

    if trezors.is_empty() {
        println!("  No Trezor devices found.");
        println!();
        println!("  Troubleshooting:");
        println!("  - Is the device connected via USB?");
        println!("  - Is trezord (Trezor Bridge) running?");
        println!("  - Try: lsusb | grep -i trezor");
    } else {
        for device in &trezors {
            print_device_details(device);
        }
    }
}

/// Non-device test: validate PSBT structure and policy extraction for BitBox02.
/// Run: TEST_PSBT=... TEST_DESCRIPTOR=... cargo test -p sigvault-desktop \
///   --test hwi_integration test_validate_psbt_for_bitbox_offline -- --nocapture
#[test]
fn test_validate_psbt_for_bitbox_offline() {
    use async_hwi::bitbox::extract_script_config_policy;
    use bitcoin::base64::{engine::general_purpose::STANDARD, Engine};
    use bitcoin::Psbt;

    let psbt_b64 = std::env::var("TEST_PSBT").unwrap_or_else(|_| DEFAULT_TEST_PSBT.to_string());
    let descriptor = std::env::var("TEST_DESCRIPTOR").unwrap_or_default();
    if descriptor.is_empty() {
        println!("TEST_DESCRIPTOR not set, skipping offline validation");
        return;
    }

    println!("\n=== OFFLINE PSBT VALIDATION FOR BITBOX02 ===\n");

    // 1. Extract policy from descriptor
    println!("--- Policy Extraction ---");
    let policy = extract_script_config_policy(&descriptor).unwrap();
    println!("  Template: {}", policy.template);
    println!("  Keys: {}", policy.pubkeys.len());
    for (i, key) in policy.pubkeys.iter().enumerate() {
        let xpub_str = key.xpub.to_string();
        println!(
            "    key[{}]: fp={:?} path={:?} xpub={}...{}",
            i,
            key.master_fingerprint,
            key.path,
            &xpub_str[..20.min(xpub_str.len())],
            &xpub_str[xpub_str.len().saturating_sub(10)..]
        );
    }

    // 2. Parse PSBT and descriptor key origins
    println!("\n--- PSBT Derivation Paths ---");
    let key_origins = parse_descriptor_key_origins(&descriptor);
    let psbt_bytes = STANDARD.decode(&psbt_b64).unwrap();
    let psbt = Psbt::deserialize(&psbt_bytes).unwrap();

    // 3. Simulate find_our_key for each fingerprint
    for (fp, account_path) in &key_origins {
        let fp_bytes = fp.as_bytes();
        println!("\n--- Simulating find_our_key for fp={} account={} ---", fp, account_path);

        for (i, input) in psbt.inputs.iter().enumerate() {
            print!("  Input #{}: ", i);
            let mut found = false;
            for (xonly, (leaf_hashes, (input_fp, path))) in &input.tap_key_origins {
                if input_fp.as_bytes() == fp_bytes {
                    if let Some(ref tap_ik) = input.tap_internal_key {
                        if tap_ik == xonly && leaf_hashes.is_empty() {
                            let path_vec: Vec<ChildNumber> = path.clone().into();
                            let acct_vec: Vec<ChildNumber> = account_path.clone().into();
                            let prefix_ok = path_vec.len() == acct_vec.len() + 2
                                && path_vec[..acct_vec.len()] == acct_vec[..];
                            println!("TaprootInternal path={} prefix_ok={}", path, prefix_ok);
                            found = true;
                            break;
                        }
                    }
                    println!("TaprootScript path={} leaves={}", path, leaf_hashes.len());
                    found = true;
                    break;
                }
            }
            if !found { println!("KeyNotFound (expected for other signer)"); }
        }

        for (i, (tx_out, output)) in psbt.unsigned_tx.output.iter().zip(psbt.outputs.iter()).enumerate() {
            print!("  Output #{}: ", i);
            let mut found = false;
            for (xonly, (leaf_hashes, (output_fp, path))) in &output.tap_key_origins {
                if output_fp.as_bytes() == fp_bytes {
                    if let Some(ref tap_ik) = output.tap_internal_key {
                        if tap_ik == xonly && leaf_hashes.is_empty() {
                            let path_vec: Vec<ChildNumber> = path.clone().into();
                            let acct_vec: Vec<ChildNumber> = account_path.clone().into();
                            let prefix_ok = path_vec.len() == acct_vec.len() + 2
                                && path_vec[..acct_vec.len()] == acct_vec[..];
                            println!("INTERNAL path={} prefix_ok={}", path, prefix_ok);
                            found = true;
                            break;
                        }
                    }
                    println!("INTERNAL(script) path={} leaves={}", path, leaf_hashes.len());
                    found = true;
                    break;
                }
            }
            if !found {
                println!("EXTERNAL {} sats type={}",
                    tx_out.value.to_sat(),
                    if tx_out.script_pubkey.is_p2tr() { "P2TR" } else { "other" });
            }
        }
    }

    println!("\n--- Checks ---");
    let locktime = psbt.unsigned_tx.lock_time.to_consensus_u32();
    let version = psbt.unsigned_tx.version.0;
    println!("  version={} locktime={} inputs={} outputs={}", version, locktime, psbt.inputs.len(), psbt.outputs.len());
    assert!(locktime < 500_000_000, "locktime >= 500000000");
    assert!(version == 1 || version == 2, "version not 1 or 2");
    for (i, input) in psbt.inputs.iter().enumerate() {
        assert!(input.witness_utxo.is_some(), "input #{} missing witness_utxo", i);
        if let Some(ref utxo) = input.witness_utxo {
            assert!(utxo.value.to_sat() > 0, "input #{} zero value", i);
        }
    }
    println!("  All checks passed.");
    println!("\n=== END OFFLINE VALIDATION ===\n");
}
