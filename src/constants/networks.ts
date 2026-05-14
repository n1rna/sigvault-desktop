// Single source of truth for the supported Bitcoin networks in
// standalone-wallet (local) mode.
//
// Mirrors the Rust-side gate in `src-tauri/src/local_wallet/settings.rs`
// (`network_key`, `ensure_supported_network`). Mainnet is intentionally
// not in this list — local-mode v1 is non-mainnet only. To enable
// mainnet later, add it here AND drop the rejection in
// `manager::ensure_supported_network` AND add a `bitcoin` electrs URL
// default in `LocalSettings::default`.

export interface NetworkOption {
	id: string;
	label: string;
	hint: string;
}

export const SUPPORTED_NETWORKS: readonly NetworkOption[] = [
	{ id: "regtest", label: "Regtest", hint: "Local dev / integration" },
	{ id: "signet", label: "Signet", hint: "Public test network" },
	{ id: "testnet4", label: "Testnet 4", hint: "Newer public testnet" },
];

export const SUPPORTED_NETWORK_IDS: readonly string[] = SUPPORTED_NETWORKS.map((n) => n.id);

/** Single feature flag the rest of the UI consults rather than hard-
 * coding a mainnet check. Currently always false — flip in lockstep
 * with the Rust-side gate when mainnet support lands. */
export const MAINNET_ENABLED = false;
