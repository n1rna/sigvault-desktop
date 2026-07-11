// Receive screen (QBL-228).
//
// Renders the wallet's external keychain index 0 address as both QR and
// monospace text. v1 always shows the same address — next-unused / per-
// index pickers land in a later UX pass. Hardware-wallet "verify on
// device" hooks in once QBL-220 wires HW into local mode.

import { invoke } from "@tauri-apps/api/core";
import { QRCodeSVG } from "qrcode.react";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Badge } from "../../components/ui/badge";
import type { LocalReceiveAddress, LocalWalletSummary } from "../../types/events";

export default function ReceiveScreen() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	const [address, setAddress] = useState<LocalReceiveAddress | null>(null);
	const [wallet, setWallet] = useState<LocalWalletSummary | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [copied, setCopied] = useState(false);

	useEffect(() => {
		if (!walletId) return;
		(async () => {
			try {
				const [addr, wallets] = await Promise.all([
					invoke<LocalReceiveAddress>("cmd_local_get_receive_address", {
						walletId,
					}),
					invoke<LocalWalletSummary[]>("cmd_local_list_wallets"),
				]);
				setAddress(addr);
				setWallet(wallets.find((w) => w.id === walletId) ?? null);
			} catch (err) {
				setError(typeof err === "string" ? err : "Failed to load address");
			} finally {
				setLoading(false);
			}
		})();
	}, [walletId]);

	const copyAddress = async () => {
		if (!address) return;
		try {
			await navigator.clipboard.writeText(address.address);
			setCopied(true);
			setTimeout(() => setCopied(false), 1800);
		} catch {
			// clipboard unavailable
		}
	};

	return (
		<div className="flex h-full w-full flex-col overflow-hidden">
			<header className="flex shrink-0 items-center gap-3 border-b border-border bg-card/60 px-6 py-3">
				<button
					type="button"
					onClick={() => navigate(`/local/wallets/${walletId}`)}
					className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					title="Back to wallet"
				>
					<svg
						className="h-4 w-4"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						<path d="m15 18-6-6 6-6" />
					</svg>
				</button>
				<div className="flex items-center gap-2.5">
					<span className="text-[14px] font-semibold text-foreground">Receive</span>
					{wallet && (
						<span className="text-[12px] text-muted-foreground">
							{wallet.name} · {wallet.network}
						</span>
					)}
				</div>
			</header>

			<div className="flex-1 overflow-y-auto px-6 py-8">
				<div className="mx-auto w-full max-w-md">
					{loading && (
						<div className="rounded-md border border-border bg-card px-4 py-3 text-[13px] text-muted-foreground">
							Loading address…
						</div>
					)}

					{error && (
						<div className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
							<svg
								className="mt-0.5 h-3.5 w-3.5 shrink-0"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<circle cx="12" cy="12" r="10" />
								<line x1="12" y1="8" x2="12" y2="12" />
								<line x1="12" y1="16" x2="12.01" y2="16" />
							</svg>
							<span className="leading-snug">{error}</span>
						</div>
					)}

					{address && (
						<div className="space-y-5">
							<div className="flex items-center justify-center rounded-lg border border-border bg-card p-6">
								<div className="rounded-md bg-white p-4">
									<QRCodeSVG value={address.address} size={224} level="M" marginSize={0} />
								</div>
							</div>

							<button
								type="button"
								onClick={copyAddress}
								title="Click to copy"
								className="group block w-full rounded-md border border-border bg-card p-4 text-left transition-colors hover:border-primary/50"
							>
								<div className="flex items-center justify-between gap-3">
									<div className="flex flex-col">
										<Badge variant="outline" className="self-start">
											{address.keychain} · index {address.index}
										</Badge>
										<span className="mt-2 break-all font-mono text-[12px] leading-relaxed text-foreground">
											{address.address}
										</span>
									</div>
									<span className="text-[12px] text-muted-foreground transition-colors group-hover:text-primary">
										{copied ? "✓ Copied" : "Copy"}
									</span>
								</div>
							</button>

							<div className="rounded-md border border-border bg-card/40 px-4 py-3">
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									This address can be reused safely but each reuse weakens your privacy. Per-index
									and next-unused address pickers ship in a later UX pass.
								</p>
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
