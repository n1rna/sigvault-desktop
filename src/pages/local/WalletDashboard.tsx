// Per-wallet dashboard placeholder.
//
// Full implementation (balance, history, sync status, receive/send CTAs)
// lands in QBL-227. This shell renders the wallet id from the route param
// and a back link so the navigation graph is wired end-to-end while the
// real dashboard is still in flight.

import { useNavigate, useParams } from "react-router-dom";

export default function WalletDashboard() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	return (
		<div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
			<h1 className="text-2xl font-semibold tracking-tight">
				Wallet dashboard
			</h1>
			<p className="max-w-md text-sm text-muted-foreground">
				Real balance, history, sync status, and receive/send actions land in
				QBL-227.
			</p>
			<p className="font-mono text-xs text-muted-foreground/70">
				wallet · {walletId}
			</p>
			<button
				type="button"
				onClick={() => navigate("/local/wallets")}
				className="text-xs text-muted-foreground underline-offset-4 hover:underline"
			>
				← Back to wallets
			</button>
		</div>
	);
}
