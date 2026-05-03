// Receive screen placeholder.
//
// Real address + QR + verify-on-device flow lands in QBL-228. This shell
// exists so the dashboard's Receive CTA has a navigation target during
// the transition.

import { useNavigate, useParams } from "react-router-dom";

export default function ReceiveScreen() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	return (
		<div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
			<h1 className="text-2xl font-semibold tracking-tight">Receive</h1>
			<p className="max-w-md text-sm text-muted-foreground">
				Address display, QR, and on-device address verification land in
				QBL-228.
			</p>
			<button
				type="button"
				onClick={() => navigate(`/local/wallets/${walletId}`)}
				className="text-xs text-muted-foreground underline-offset-4 hover:underline"
			>
				← Back to wallet
			</button>
		</div>
	);
}
