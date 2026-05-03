// Pre-login mode chooser placeholder.
//
// QBL-214 wires the routing + state plumbing for two top-level app modes
// (Cloud vs Local). The actual chooser UI — branded cards, copy, design
// polish — is QBL-221. This placeholder verifies the round-trip works end
// to end (cmd_set_app_mode persists, init flow short-circuits correctly,
// AppStateContext reflects the choice).

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppMode } from "../types/events";

export default function ModeChooser() {
	const [busy, setBusy] = useState<AppMode | null>(null);
	const [error, setError] = useState<string | null>(null);

	const choose = async (mode: AppMode) => {
		setError(null);
		setBusy(mode);
		try {
			await invoke("cmd_set_app_mode", { mode });
		} catch (e) {
			setError(typeof e === "string" ? e : "Failed to set mode");
			setBusy(null);
		}
	};

	return (
		<div className="flex h-full items-center justify-center p-8">
			<div className="w-full max-w-2xl space-y-6">
				<div className="space-y-2 text-center">
					<h1 className="text-3xl font-semibold tracking-tight">
						Choose how to use SigVault
					</h1>
					<p className="text-muted-foreground">
						You can switch later from Settings.
					</p>
				</div>

				<div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
					<button
						type="button"
						disabled={busy != null}
						onClick={() => choose("cloud")}
						className="rounded-lg border border-border bg-card p-6 text-left transition-colors hover:bg-muted disabled:opacity-50"
					>
						<div className="text-lg font-medium">SigVault Cloud</div>
						<div className="mt-2 text-sm text-muted-foreground">
							Sign in to coordinate multisig with your team.
						</div>
						{busy === "cloud" && (
							<div className="mt-3 text-xs text-primary">Setting up…</div>
						)}
					</button>

					<button
						type="button"
						disabled={busy != null}
						onClick={() => choose("local")}
						className="rounded-lg border border-border bg-card p-6 text-left transition-colors hover:bg-muted disabled:opacity-50"
					>
						<div className="text-lg font-medium">Local Wallet</div>
						<div className="mt-2 text-sm text-muted-foreground">
							Use SigVault as a fully-local Bitcoin wallet — no account, no cloud.
						</div>
						{busy === "local" && (
							<div className="mt-3 text-xs text-primary">Setting up…</div>
						)}
					</button>
				</div>

				{error && (
					<div className="rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
						{error}
					</div>
				)}
			</div>
		</div>
	);
}
