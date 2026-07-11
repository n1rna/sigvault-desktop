// Local wallet list (QBL-222).
//
// Lists all on-disk wallets discovered by `cmd_local_list_wallets`,
// supports unlock / lock / delete, and routes to the create wizard
// (QBL-223) and per-wallet dashboard (QBL-227). Once unlocked, opening a
// wallet auto-syncs in the background; the dashboard wires the progress
// hook for the visible status indicator.

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import type { LocalWalletSummary } from "../../types/events";

type ModalState =
	| { kind: "none" }
	| { kind: "unlock"; wallet: LocalWalletSummary }
	| { kind: "delete"; wallet: LocalWalletSummary };

function formatNetwork(n: string) {
	return n.toUpperCase();
}

function formatPolicyType(t: string) {
	switch (t) {
		case "singlesig_hot":
			return "Singlesig · hot";
		case "multisig":
			return "Multisig";
		case "timelocked":
			return "Timelocked policy";
		case "watch_only":
			return "Watch-only";
		default:
			return t;
	}
}

function formatCreatedAt(unix: number) {
	const d = new Date(unix * 1000);
	return d.toLocaleDateString(undefined, {
		year: "numeric",
		month: "short",
		day: "numeric",
	});
}

export default function LocalWalletList() {
	const [wallets, setWallets] = useState<LocalWalletSummary[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [modal, setModal] = useState<ModalState>({ kind: "none" });
	const navigate = useNavigate();

	const loadWallets = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const list = await invoke<LocalWalletSummary[]>("cmd_local_list_wallets");
			list.sort((a, b) => b.created_at - a.created_at);
			setWallets(list);
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to load wallets");
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		loadWallets();
	}, [loadWallets]);

	const openWallet = async (wallet: LocalWalletSummary) => {
		if (wallet.locked && wallet.has_hot_keys) {
			// Hot wallets need the passphrase to decrypt seed.enc.
			setModal({ kind: "unlock", wallet });
			return;
		}
		if (wallet.locked) {
			// HW / watch-only wallets have no on-disk seed; unlock just
			// loads the BDK store + kicks off auto-sync. Empty passphrase.
			try {
				await invoke("cmd_local_unlock_wallet", {
					request: { wallet_id: wallet.id, passphrase: "" },
				});
			} catch (err) {
				setError(typeof err === "string" ? err : "Failed to open wallet");
				return;
			}
		}
		navigate(`/local/wallets/${wallet.id}`);
	};

	const lockWallet = async (wallet: LocalWalletSummary) => {
		try {
			await invoke("cmd_local_lock_wallet", { walletId: wallet.id });
			await loadWallets();
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to lock wallet");
		}
	};

	return (
		<div className="h-full w-full overflow-y-auto">
			<div className="mx-auto flex max-w-3xl flex-col gap-6 px-10 py-9">
				{/* ── Header ── */}
				<div
					className="flex items-start justify-between gap-4"
					onMouseDown={(e) => e.stopPropagation()}
				>
					<div>
						<h1 className="text-[20px] font-semibold tracking-tight text-foreground">
							Local wallets
						</h1>
						<p className="mt-1 text-[13px] text-muted-foreground">
							On-device wallets. Your seed never leaves this machine.
						</p>
					</div>

					<div className="flex shrink-0 items-center gap-2">
						<Button
							variant="outline"
							size="icon"
							onClick={() => navigate("/local/settings")}
							title="Settings"
						>
							<svg
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<circle cx="12" cy="12" r="3" />
								<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
							</svg>
						</Button>
						<Button variant="outline" size="sm" onClick={loadWallets} disabled={loading}>
							Refresh
						</Button>
						<Button variant="outline" size="sm" onClick={() => navigate("/local/wallets/recover")}>
							Recover
						</Button>
						<Button size="sm" onClick={() => navigate("/local/wallets/new")}>
							<svg
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2.5"
								strokeLinecap="round"
							>
								<path d="M12 5v14M5 12h14" />
							</svg>
							New wallet
						</Button>
					</div>
				</div>

				{/* ── Body ── */}
				<div onMouseDown={(e) => e.stopPropagation()}>
					{error && (
						<div className="mb-4 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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

					{loading ? (
						<div className="rounded-lg border border-border bg-card px-4 py-3 text-[13px] text-muted-foreground">
							Loading wallets…
						</div>
					) : wallets.length === 0 ? (
						<EmptyState onCreate={() => navigate("/local/wallets/new")} />
					) : (
						<ul className="flex flex-col gap-2.5">
							{wallets.map((w) => (
								<WalletCard
									key={w.id}
									wallet={w}
									onOpen={() => openWallet(w)}
									onLock={() => lockWallet(w)}
									onDelete={() => setModal({ kind: "delete", wallet: w })}
								/>
							))}
						</ul>
					)}
				</div>
			</div>

			{modal.kind === "unlock" && (
				<UnlockModal
					wallet={modal.wallet}
					onClose={() => setModal({ kind: "none" })}
					onUnlocked={async () => {
						setModal({ kind: "none" });
						await loadWallets();
						navigate(`/local/wallets/${modal.wallet.id}`);
					}}
				/>
			)}

			{modal.kind === "delete" && (
				<DeleteModal
					wallet={modal.wallet}
					onClose={() => setModal({ kind: "none" })}
					onDeleted={async () => {
						setModal({ kind: "none" });
						await loadWallets();
					}}
				/>
			)}
		</div>
	);
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
	return (
		<div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border bg-card/40 px-10 py-16 text-center">
			<div className="flex h-12 w-12 items-center justify-center rounded-full border border-border bg-background">
				<svg
					className="h-5 w-5 text-muted-foreground"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="1.5"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="M19 7H5a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2Z" />
					<path d="M16 11h.01" />
					<path d="M3 9V7a2 2 0 0 1 2-2h12" />
				</svg>
			</div>
			<h2 className="mt-5 text-[16px] font-semibold tracking-tight text-foreground">
				No wallets yet
			</h2>
			<p className="mt-2 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
				Create your first local wallet to start receiving and signing transactions on-device. Your
				seed never leaves this machine.
			</p>
			<Button className="mt-6" onClick={onCreate}>
				<svg
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2.5"
					strokeLinecap="round"
				>
					<path d="M12 5v14M5 12h14" />
				</svg>
				Create wallet
			</Button>
		</div>
	);
}

function WalletCard({
	wallet,
	onOpen,
	onLock,
	onDelete,
}: {
	wallet: LocalWalletSummary;
	onOpen: () => void;
	onLock: () => void;
	onDelete: () => void;
}) {
	const fp = wallet.fingerprints[0];
	return (
		<li className="group relative rounded-lg border border-border bg-card transition-colors hover:border-primary/50">
			<button
				type="button"
				onClick={onOpen}
				className="flex w-full items-center justify-between px-4 py-3.5 text-left"
			>
				<div className="flex items-center gap-3.5">
					<div className="flex h-9 w-9 items-center justify-center rounded-md border border-border bg-background">
						{wallet.locked ? (
							<svg
								className="h-3.5 w-3.5 text-muted-foreground"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<rect x="3" y="11" width="18" height="11" rx="2" />
								<path d="M7 11V7a5 5 0 0 1 10 0v4" />
							</svg>
						) : (
							<svg
								className="h-3.5 w-3.5 text-success"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<rect x="3" y="11" width="18" height="11" rx="2" />
								<path d="M7 11V7a5 5 0 0 1 9.9-1" />
							</svg>
						)}
					</div>
					<div className="flex flex-col">
						<div className="flex items-center gap-2">
							<span className="text-[14px] font-medium text-foreground">{wallet.name}</span>
							<Badge variant="outline">{formatNetwork(wallet.network)}</Badge>
							{wallet.recovery_only && <Badge variant="warning">Recovery-only</Badge>}
						</div>
						<div className="mt-1 flex items-center gap-2 text-[12px] text-muted-foreground">
							<span>{formatPolicyType(wallet.policy_type)}</span>
							{fp && (
								<>
									<span className="h-1 w-1 rounded-full bg-muted-foreground/40" />
									<span className="font-mono">{fp}</span>
								</>
							)}
							<span className="h-1 w-1 rounded-full bg-muted-foreground/40" />
							<span>{formatCreatedAt(wallet.created_at)}</span>
						</div>
					</div>
				</div>
				<svg
					className="h-4 w-4 text-muted-foreground transition-transform group-hover:translate-x-0.5"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="m9 18 6-6-6-6" />
				</svg>
			</button>
			<div className="absolute right-11 top-1/2 -translate-y-1/2 opacity-0 transition-opacity group-hover:opacity-100">
				<div className="flex items-center gap-1">
					{!wallet.locked && (
						<button
							type="button"
							onClick={(e) => {
								e.stopPropagation();
								onLock();
							}}
							title="Lock wallet"
							className="flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
						>
							<svg
								className="h-3.5 w-3.5"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<rect x="3" y="11" width="18" height="11" rx="2" />
								<path d="M7 11V7a5 5 0 0 1 10 0v4" />
							</svg>
						</button>
					)}
					<button
						type="button"
						onClick={(e) => {
							e.stopPropagation();
							onDelete();
						}}
						title="Delete wallet"
						className="flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
					>
						<svg
							className="h-3.5 w-3.5"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<path d="M3 6h18" />
							<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
							<path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
						</svg>
					</button>
				</div>
			</div>
		</li>
	);
}

function UnlockModal({
	wallet,
	onClose,
	onUnlocked,
}: {
	wallet: LocalWalletSummary;
	onClose: () => void;
	onUnlocked: () => void;
}) {
	const [passphrase, setPassphrase] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const submit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (!passphrase) return;
		setSubmitting(true);
		setError(null);
		try {
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: wallet.id, passphrase },
			});
			onUnlocked();
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to unlock wallet");
			setSubmitting(false);
		}
	};

	return (
		<ModalShell title="Unlock wallet" subtitle={wallet.name} onClose={onClose}>
			<form onSubmit={submit} className="space-y-4">
				<label className="block">
					<span className="text-[12px] font-medium text-muted-foreground">Passphrase</span>
					<input
						type="password"
						value={passphrase}
						onChange={(e) => setPassphrase(e.target.value)}
						className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
					/>
				</label>
				{error && (
					<div className="rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive">
						{error}
					</div>
				)}
				<Button type="submit" size="lg" disabled={submitting || !passphrase} className="w-full">
					{submitting ? "Unlocking…" : "Unlock"}
				</Button>
			</form>
		</ModalShell>
	);
}

function DeleteModal({
	wallet,
	onClose,
	onDeleted,
}: {
	wallet: LocalWalletSummary;
	onClose: () => void;
	onDeleted: () => void;
}) {
	const [passphrase, setPassphrase] = useState("");
	const [confirmName, setConfirmName] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const requirePassphrase = wallet.has_hot_keys;
	const ready = confirmName === wallet.name && (!requirePassphrase || !!passphrase);

	const submit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (!ready) return;
		setSubmitting(true);
		setError(null);
		try {
			await invoke("cmd_local_delete_wallet", {
				request: {
					wallet_id: wallet.id,
					passphrase: requirePassphrase ? passphrase : null,
				},
			});
			onDeleted();
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to delete wallet");
			setSubmitting(false);
		}
	};

	return (
		<ModalShell title="Delete wallet" subtitle={wallet.name} onClose={onClose}>
			<form onSubmit={submit} className="space-y-4">
				<p className="text-[12px] leading-relaxed text-muted-foreground">
					This permanently removes the on-disk wallet, including the encrypted seed. Recovery
					requires the original BIP39 mnemonic. Type the wallet name to confirm.
				</p>
				<label className="block">
					<span className="text-[12px] font-medium text-muted-foreground">Confirm name</span>
					<input
						type="text"
						value={confirmName}
						onChange={(e) => setConfirmName(e.target.value)}
						placeholder={wallet.name}
						className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
					/>
				</label>
				{requirePassphrase && (
					<label className="block">
						<span className="text-[12px] font-medium text-muted-foreground">Passphrase</span>
						<input
							type="password"
							value={passphrase}
							onChange={(e) => setPassphrase(e.target.value)}
							className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
						/>
					</label>
				)}
				{error && (
					<div className="rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-[12px] text-destructive">
						{error}
					</div>
				)}
				<Button
					type="submit"
					variant="destructive"
					size="lg"
					disabled={submitting || !ready}
					className="w-full"
				>
					{submitting ? "Deleting…" : "Delete wallet"}
				</Button>
			</form>
		</ModalShell>
	);
}

function ModalShell({
	title,
	subtitle,
	onClose,
	children,
}: {
	title: string;
	subtitle?: string;
	onClose: () => void;
	children: React.ReactNode;
}) {
	return (
		<div
			className="fixed inset-0 z-40 flex items-center justify-center bg-background/70 backdrop-blur-sm"
			onMouseDown={onClose}
		>
			<div
				className="w-full max-w-md rounded-lg border border-border bg-card p-6 shadow-2xl"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="mb-5 flex items-start justify-between">
					<div>
						<h2 className="text-[16px] font-semibold tracking-tight text-foreground">{title}</h2>
						{subtitle && <p className="mt-1 text-[12px] text-muted-foreground">{subtitle}</p>}
					</div>
					<button
						type="button"
						onClick={onClose}
						className="flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					>
						<svg
							className="h-3.5 w-3.5"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
						>
							<path d="M18 6 6 18M6 6l12 12" />
						</svg>
					</button>
				</div>
				{children}
			</div>
		</div>
	);
}
