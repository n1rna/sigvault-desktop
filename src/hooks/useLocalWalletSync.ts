// Subscribes to backend `local_wallet_sync_progress` events for a single
// wallet. The dashboard (QBL-227) and wallet list (QBL-222) drive
// per-wallet sync indicators off this hook; the underlying event channel
// is shared across all wallets, so the hook filters by wallet_id.

import { useEffect, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
	LocalWalletSyncPhase,
	LocalWalletSyncProgress,
} from "../types/events";

export type LocalSyncStatus = "idle" | "active" | "complete" | "error";

export interface LocalSyncState {
	status: LocalSyncStatus;
	phase: LocalWalletSyncPhase | null;
	percent: number;
	message: string;
	/** Populated when the sync surface emits an error event. Cleared when
	 * a fresh sync starts (a `connecting` event resets state). */
	error: string | null;
}

const initial: LocalSyncState = {
	status: "idle",
	phase: null,
	percent: 0,
	message: "",
	error: null,
};

const SYNC_CHANNEL = "local_wallet_sync_progress";

/**
 * Listen on the backend sync-progress channel and surface state for a
 * single wallet. Returns the latest `{ status, phase, percent, message,
 * error }` snapshot the UI can render directly. Idle until the first
 * progress event for `walletId` arrives; resets to idle if `walletId`
 * changes.
 */
export function useLocalWalletSync(
	walletId: string | null | undefined,
): LocalSyncState {
	const [state, setState] = useState<LocalSyncState>(initial);

	useEffect(() => {
		if (!walletId) {
			setState(initial);
			return;
		}
		// Drop any stale state from a previous wallet selection.
		setState(initial);

		let unlisten: UnlistenFn | null = null;
		let cancelled = false;

		listen<LocalWalletSyncProgress>(SYNC_CHANNEL, (event) => {
			const p = event.payload;
			if (p.wallet_id !== walletId) return;

			setState({
				status: p.phase === "complete" ? "complete" : "active",
				phase: p.phase,
				percent: p.percent,
				message: p.message,
				error: null,
			});
		}).then((fn) => {
			if (cancelled) {
				fn();
				return;
			}
			unlisten = fn;
		});

		return () => {
			cancelled = true;
			if (unlisten) unlisten();
		};
	}, [walletId]);

	return state;
}
