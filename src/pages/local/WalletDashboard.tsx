// Per-wallet dashboard (QBL-227).
//
// Renders balance, transaction history, and sync status for a single
// unlocked wallet. Auto-refreshes balance + history when the sync hook
// reports completion. Receive (QBL-228) and Send (QBL-229) actions land
// next; the buttons here route to placeholder pages so the navigation
// graph stays whole during the transition.

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { useLocalWalletSync } from "../../hooks/useLocalWalletSync";
import type { LocalBalance, LocalWalletSummary } from "../../types/events";

interface LocalTxRecord {
	txid: string;
	net_sat: number;
	fee_sat: number;
	confirmed: boolean;
	block_height: number | null;
	block_time: number | null;
}

const SAT_PER_BTC = 100_000_000;

function formatBtc(sat: number) {
	const sign = sat < 0 ? "-" : "";
	const abs = Math.abs(sat);
	const whole = Math.floor(abs / SAT_PER_BTC);
	const frac = (abs % SAT_PER_BTC).toString().padStart(8, "0");
	return `${sign}${whole}.${frac}`;
}

function formatRelative(unix: number | null) {
	if (!unix) return "—";
	const now = Date.now() / 1000;
	const diff = now - unix;
	if (diff < 60) return "just now";
	if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
	if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
	if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}d ago`;
	return new Date(unix * 1000).toLocaleDateString(undefined, {
		month: "short",
		day: "numeric",
		year: "numeric",
	});
}

function shortTxid(txid: string) {
	if (txid.length <= 16) return txid;
	return `${txid.slice(0, 8)}…${txid.slice(-6)}`;
}

export default function WalletDashboard() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	const [wallet, setWallet] = useState<LocalWalletSummary | null>(null);
	const [balance, setBalance] = useState<LocalBalance | null>(null);
	const [history, setHistory] = useState<LocalTxRecord[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [syncing, setSyncing] = useState(false);
	const [copiedTxid, setCopiedTxid] = useState<string | null>(null);
	const sync = useLocalWalletSync(walletId ?? null);

	const loadAll = useCallback(async () => {
		if (!walletId) return;
		setError(null);
		try {
			const [wallets, bal, hist] = await Promise.all([
				invoke<LocalWalletSummary[]>("cmd_local_list_wallets"),
				invoke<LocalBalance>("cmd_local_get_balance", { walletId }),
				invoke<LocalTxRecord[]>("cmd_local_get_history", { walletId }),
			]);
			const found = wallets.find((w) => w.id === walletId) ?? null;
			setWallet(found);
			setBalance(bal);
			setHistory(hist);
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to load wallet");
		} finally {
			setLoading(false);
		}
	}, [walletId]);

	useEffect(() => {
		loadAll();
	}, [loadAll]);

	// Refresh balance + history when sync completes (auto-sync after
	// unlock, or a manual trigger). The hook's `status` toggles to
	// "complete" exactly once per sync cycle.
	useEffect(() => {
		if (sync.status === "complete") {
			loadAll();
		}
	}, [sync.status, loadAll]);

	const triggerSync = async () => {
		if (!walletId || syncing) return;
		setSyncing(true);
		setError(null);
		try {
			await invoke("cmd_local_sync", { walletId });
		} catch (err) {
			setError(typeof err === "string" ? err : "Sync failed");
		} finally {
			setSyncing(false);
		}
	};

	const lockAndExit = async () => {
		if (!walletId) return;
		try {
			await invoke("cmd_local_lock_wallet", { walletId });
		} finally {
			navigate("/local/wallets");
		}
	};

	const copyTxid = async (txid: string) => {
		try {
			await navigator.clipboard.writeText(txid);
			setCopiedTxid(txid);
			setTimeout(() => setCopiedTxid(null), 1500);
		} catch {
			// clipboard unavailable
		}
	};

	if (loading) {
		return (
			<div className="flex h-full items-center justify-center">
				<div className="text-[13px] text-muted-foreground">Loading wallet…</div>
			</div>
		);
	}

	return (
		<div className="flex h-full w-full flex-col overflow-hidden">
			{/* ── Header bar ── */}
			<header className="flex shrink-0 items-center justify-between border-b border-border bg-card/60 px-6 py-3">
				<div className="flex items-center gap-3">
					<button
						type="button"
						onClick={() => navigate("/local/wallets")}
						className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
						title="Back to wallets"
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
						<span className="text-[14px] font-semibold text-foreground">
							{wallet?.name ?? "Wallet"}
						</span>
						{wallet && (
							<>
								<Badge variant="outline">{wallet.network.toUpperCase()}</Badge>
								<span className="text-[12px] text-muted-foreground">{wallet.policy_type}</span>
							</>
						)}
					</div>
				</div>

				<div className="flex items-center gap-2">
					<SyncIndicator sync={sync} syncing={syncing} onSync={triggerSync} />
					<Button variant="outline" size="sm" onClick={lockAndExit}>
						<svg
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
						Lock
					</Button>
				</div>
			</header>

			{/* ── Body ── */}
			<div className="flex-1 overflow-y-auto px-6 py-7">
				<div className="mx-auto max-w-3xl space-y-6">
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

					<BalanceCard balance={balance} />

					{wallet?.recovery_only && (
						<div className="flex items-start gap-2.5 rounded-md border border-warning/30 bg-warning/[0.05] px-3.5 py-3 text-[12px] text-foreground">
							<svg
								className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M12 8v4" />
								<path d="M12 16h.01" />
								<circle cx="12" cy="12" r="10" />
							</svg>
							<div className="leading-snug">
								<div className="font-medium">Recovery-only wallet</div>
								<div className="mt-0.5 text-muted-foreground">
									The primary path is unspendable. Funds can only be moved via the recovery key
									after the timelock elapses.
								</div>
							</div>
						</div>
					)}

					<div
						className={
							wallet?.policy_type === "watch_only" ? "grid grid-cols-1" : "grid grid-cols-2 gap-3"
						}
					>
						<Button
							variant="outline"
							className="h-12"
							onClick={() => navigate(`/local/wallets/${walletId}/receive`)}
						>
							<svg
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M19 12H5" />
								<path d="m12 19-7-7 7-7" />
							</svg>
							Receive
						</Button>
						{wallet?.policy_type !== "watch_only" && (
							<Button
								variant="outline"
								className="h-12"
								onClick={() => navigate(`/local/wallets/${walletId}/send`)}
							>
								<svg
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<path d="M5 12h14" />
									<path d="m12 5 7 7-7 7" />
								</svg>
								Send
							</Button>
						)}
					</div>

					<HistoryList history={history} copiedTxid={copiedTxid} onCopyTxid={copyTxid} />
				</div>
			</div>
		</div>
	);
}

function BalanceCard({ balance }: { balance: LocalBalance | null }) {
	const confirmed = balance?.confirmed_sat ?? 0;
	const pending = balance?.unconfirmed_sat ?? 0;
	return (
		<div className="rounded-lg border border-border bg-card px-6 py-6">
			<div className="text-[12px] font-medium text-muted-foreground">Balance</div>
			<div className="mt-2 flex items-baseline gap-2.5">
				<div className="font-mono text-[36px] font-medium leading-none tracking-tight tabular-nums text-foreground">
					{formatBtc(confirmed)}
				</div>
				<span className="text-[13px] text-muted-foreground">BTC</span>
			</div>
			<div className="mt-2 font-mono text-[11px] tabular-nums text-muted-foreground">
				{confirmed.toLocaleString()} sat
			</div>
			{pending > 0 && (
				<div className="mt-3 flex items-center gap-2 text-[12px] text-warning">
					<span className="inline-flex h-1.5 w-1.5 rounded-full bg-warning" />
					<span>
						+<span className="font-mono tabular-nums">{formatBtc(pending)}</span> BTC pending
					</span>
				</div>
			)}
		</div>
	);
}

function SyncIndicator({
	sync,
	syncing,
	onSync,
}: {
	sync: ReturnType<typeof useLocalWalletSync>;
	syncing: boolean;
	onSync: () => void;
}) {
	const active = syncing || sync.status === "active";
	const label = active
		? sync.message || "Syncing…"
		: sync.status === "complete"
			? "Synced"
			: "Sync";
	return (
		<Button
			variant="ghost"
			size="sm"
			onClick={onSync}
			disabled={active}
			title={active ? `${sync.phase ?? ""} (${sync.percent}%)` : "Run an Electrum sync"}
		>
			{active ? (
				<svg
					className="animate-spin"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2.5"
					strokeLinecap="round"
				>
					<path d="M21 12a9 9 0 1 1-6.219-8.56" />
				</svg>
			) : (
				<svg
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
					<path d="M3 3v5h5" />
					<path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
					<path d="M16 16h5v5" />
				</svg>
			)}
			{label}
		</Button>
	);
}

function HistoryList({
	history,
	copiedTxid,
	onCopyTxid,
}: {
	history: LocalTxRecord[];
	copiedTxid: string | null;
	onCopyTxid: (txid: string) => void;
}) {
	return (
		<section>
			<div className="mb-3 flex items-center justify-between">
				<div className="text-[13px] font-semibold text-foreground">Transactions</div>
				<div className="text-[12px] tabular-nums text-muted-foreground">{history.length} total</div>
			</div>
			{history.length === 0 ? (
				<div className="rounded-lg border border-dashed border-border bg-card/40 px-5 py-10 text-center">
					<p className="text-[13px] text-foreground">No transactions yet</p>
					<p className="mt-1 text-[12px] text-muted-foreground">
						Use Receive to fund this wallet, then run a sync.
					</p>
				</div>
			) : (
				<ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
					{history.map((tx) => (
						<TxRow
							key={tx.txid}
							tx={tx}
							copied={copiedTxid === tx.txid}
							onCopy={() => onCopyTxid(tx.txid)}
						/>
					))}
				</ul>
			)}
		</section>
	);
}

function TxRow({ tx, copied, onCopy }: { tx: LocalTxRecord; copied: boolean; onCopy: () => void }) {
	const incoming = tx.net_sat >= 0;
	return (
		<li className="flex items-center gap-4 px-5 py-3">
			<div
				className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${
					incoming ? "bg-success/[0.10] text-success" : "bg-destructive/[0.10] text-destructive"
				}`}
			>
				<svg
					className="h-3.5 w-3.5"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2.5"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					{incoming ? (
						<>
							<path d="m6 9 6 6 6-6" />
							<path d="M12 3v12" />
						</>
					) : (
						<>
							<path d="m18 15-6-6-6 6" />
							<path d="M12 21V9" />
						</>
					)}
				</svg>
			</div>
			<div className="flex flex-1 items-center justify-between gap-4">
				<div className="flex flex-col">
					<button
						type="button"
						onClick={onCopy}
						className="text-left font-mono text-[12px] text-foreground transition-colors hover:text-primary"
						title={tx.txid}
					>
						{copied ? "✓ Copied" : shortTxid(tx.txid)}
					</button>
					<div className="mt-0.5 flex items-center gap-2 text-[12px] text-muted-foreground">
						<span>{formatRelative(tx.block_time)}</span>
						{tx.confirmed ? (
							<>
								<span className="h-1 w-1 rounded-full bg-muted-foreground/40" />
								<span>block {tx.block_height}</span>
							</>
						) : (
							<>
								<span className="h-1 w-1 rounded-full bg-muted-foreground/40" />
								<span className="text-warning">pending</span>
							</>
						)}
						{tx.fee_sat > 0 && (
							<>
								<span className="h-1 w-1 rounded-full bg-muted-foreground/40" />
								<span>fee {tx.fee_sat.toLocaleString()} sat</span>
							</>
						)}
					</div>
				</div>
				<div
					className={`text-right font-mono text-[13px] font-medium tabular-nums ${
						incoming ? "text-success" : "text-foreground"
					}`}
				>
					{incoming ? "+" : ""}
					{formatBtc(tx.net_sat)}
					<div className="mt-0.5 text-[10px] font-normal text-muted-foreground/70">BTC</div>
				</div>
			</div>
		</li>
	);
}
