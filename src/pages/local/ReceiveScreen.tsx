// Receive screen (QBL-228).
//
// Renders the wallet's external keychain index 0 address as both QR and
// monospace text. v1 always shows the same address — next-unused / per-
// index pickers land in a later UX pass. Hardware-wallet "verify on
// device" hooks in once QBL-220 wires HW into local mode.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate, useParams } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { QRCodeSVG } from "qrcode.react";
import WindowControls from "../../components/WindowControls";
import type {
	LocalReceiveAddress,
	LocalWalletSummary,
} from "../../types/events";

export default function ReceiveScreen() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	const [address, setAddress] = useState<LocalReceiveAddress | null>(null);
	const [wallet, setWallet] = useState<LocalWalletSummary | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [copied, setCopied] = useState(false);

	const onDrag = useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			try {
				getCurrentWindow().startDragging();
			} catch {
				// no-op outside Tauri
			}
		}
	}, []);

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
		<div
			onMouseDown={onDrag}
			className="relative flex h-screen w-full select-none flex-col overflow-hidden bg-background"
		>
			<WindowControls />

			<div className="pointer-events-none absolute inset-0 bg-grid mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.10]" />

			<header
				className="relative flex shrink-0 items-center justify-between border-b border-border bg-card/60 px-8 py-4 backdrop-blur-sm"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="flex items-center gap-3">
					<button
						type="button"
						onClick={() => navigate(`/local/wallets/${walletId}`)}
						className="flex h-8 w-8 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
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
					<div className="flex flex-col leading-none">
						<span className="text-[14px] font-medium text-foreground">
							Receive
						</span>
						<span className="mt-1 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
							{wallet ? `${wallet.name} · ${wallet.network}` : "—"}
						</span>
					</div>
				</div>
			</header>

			<div
				className="relative flex flex-1 items-start justify-center overflow-y-auto px-6 py-10"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="w-full max-w-md">
					{loading && (
						<div className="rounded-md border border-border bg-card px-4 py-3 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
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
						<div className="space-y-6">
							<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
								§ — Address
							</div>
							<div className="flex items-center justify-center rounded-lg border border-border bg-card p-6">
								<div className="rounded-md bg-white p-4">
									<QRCodeSVG
										value={address.address}
										size={224}
										level="M"
										marginSize={0}
									/>
								</div>
							</div>

							<button
								type="button"
								onClick={copyAddress}
								title="Click to copy"
								className="group block w-full rounded-md border border-border bg-card p-4 text-left transition-colors hover:border-primary/60 hover:bg-primary/[0.03]"
							>
								<div className="flex items-center justify-between gap-3">
									<div className="flex flex-col">
										<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
											{address.keychain} · index {address.index}
										</span>
										<span className="mt-1 break-all font-mono text-[12px] leading-relaxed text-foreground">
											{address.address}
										</span>
									</div>
									<span className="font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground transition-colors group-hover:text-primary">
										{copied ? "✓ Copied" : "Copy"}
									</span>
								</div>
							</button>

							<div className="rounded-md border border-border bg-card/40 px-4 py-3">
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									This address can be reused safely but each reuse weakens
									your privacy. Per-index and next-unused address pickers
									ship in a later UX pass.
								</p>
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
