// Send screen placeholder.
//
// Full send flow (recipient → fee → sign → broadcast) lands in QBL-229
// once the PSBT backend (QBL-219) is in place.

import { useNavigate, useParams } from "react-router-dom";

export default function SendScreen() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	return (
		<div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
			<h1 className="text-2xl font-semibold tracking-tight">Send</h1>
			<p className="max-w-md text-sm text-muted-foreground">
				PSBT build / sign / broadcast pipeline lands in QBL-219 + QBL-229.
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
