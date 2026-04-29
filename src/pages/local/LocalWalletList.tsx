// Local wallet list placeholder.
//
// QBL-214 only verifies routing into local mode lands here. The real UI
// (wallet cards, empty state, "+ New wallet" CTA, settings) is QBL-222.

import { invoke } from "@tauri-apps/api/core";

export default function LocalWalletList() {
	const switchMode = async () => {
		await invoke("cmd_clear_app_mode");
	};

	return (
		<div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
			<h1 className="text-2xl font-semibold tracking-tight">
				Local wallets
			</h1>
			<p className="max-w-md text-sm text-muted-foreground">
				This is the placeholder for the local-mode wallet list. The real UI
				lands in QBL-222. Local mode is the standalone Bitcoin wallet
				experience — no SigVault Cloud account needed.
			</p>
			<button
				type="button"
				onClick={switchMode}
				className="text-xs text-muted-foreground underline-offset-4 hover:underline"
			>
				Switch to SigVault Cloud
			</button>
		</div>
	);
}
