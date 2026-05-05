// Send wizard (QBL-229).
//
// Three-step flow on top of the QBL-219 PSBT pipeline:
//   1. Compose — recipient address, amount, fee rate
//   2. Confirm — review summary, type passphrase, sign + broadcast
//   3. Done — show resulting txid with copy
//
// The sign and broadcast calls fire back-to-back from step 2 — once a
// PSBT is fully signed there's no reason to make the user wait between
// signing and broadcasting. If broadcast fails after a successful sign
// the wizard surfaces the error and lets the user retry from the same
// signed PSBT.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate, useParams } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import WindowControls from "../../components/WindowControls";
import type {
	LocalBalance,
	LocalBroadcastPsbtResponse,
	LocalBuildPsbtResponse,
	LocalSignPsbtResponse,
	LocalWalletSummary,
} from "../../types/events";

type Step = "compose" | "confirm" | "done";

const SAT_PER_BTC = 100_000_000;

function btcToSat(btc: string): number | null {
	const trimmed = btc.trim();
	if (!trimmed) return null;
	const parsed = Number(trimmed);
	if (!Number.isFinite(parsed) || parsed <= 0) return null;
	return Math.round(parsed * SAT_PER_BTC);
}

function formatBtc(sat: number) {
	const sign = sat < 0 ? "-" : "";
	const abs = Math.abs(sat);
	const whole = Math.floor(abs / SAT_PER_BTC);
	const frac = (abs % SAT_PER_BTC).toString().padStart(8, "0");
	return `${sign}${whole}.${frac}`;
}

export default function SendScreen() {
	const { walletId } = useParams<{ walletId: string }>();
	const navigate = useNavigate();
	const [step, setStep] = useState<Step>("compose");
	const [wallet, setWallet] = useState<LocalWalletSummary | null>(null);
	const [balance, setBalance] = useState<LocalBalance | null>(null);

	// Compose step state
	const [recipient, setRecipient] = useState("");
	const [amountBtc, setAmountBtc] = useState("");
	const [feeRate, setFeeRate] = useState("2");

	// Built PSBT (after compose)
	const [psbtBase64, setPsbtBase64] = useState<string | null>(null);

	// Confirm step state
	const [passphrase, setPassphrase] = useState("");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);

	// Done step state
	const [txid, setTxid] = useState<string | null>(null);
	const [copiedTxid, setCopiedTxid] = useState(false);

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
				const [wallets, bal] = await Promise.all([
					invoke<LocalWalletSummary[]>("cmd_local_list_wallets"),
					invoke<LocalBalance>("cmd_local_get_balance", { walletId }),
				]);
				setWallet(wallets.find((w) => w.id === walletId) ?? null);
				setBalance(bal);
			} catch {
				// non-fatal: wizard still usable, just shows fewer hints
			}
		})();
	}, [walletId]);

	const buildPsbt = async () => {
		if (!walletId) return;
		setError(null);
		const sat = btcToSat(amountBtc);
		if (!sat) {
			setError("Enter a valid amount in BTC.");
			return;
		}
		const fr = Number(feeRate);
		if (!Number.isFinite(fr) || fr <= 0) {
			setError("Fee rate must be a positive number.");
			return;
		}
		if (!recipient.trim()) {
			setError("Recipient address is required.");
			return;
		}
		setBusy(true);
		try {
			const resp = await invoke<LocalBuildPsbtResponse>("cmd_local_build_psbt", {
				request: {
					wallet_id: walletId,
					recipients: [{ address: recipient.trim(), amount_sat: sat }],
					fee_rate_sat_vb: fr,
				},
			});
			setPsbtBase64(resp.psbt_base64);
			setStep("confirm");
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to build transaction");
		} finally {
			setBusy(false);
		}
	};

	const signAndBroadcast = async () => {
		if (!walletId || !psbtBase64 || !passphrase) return;
		setError(null);
		setBusy(true);
		try {
			const signed = await invoke<LocalSignPsbtResponse>(
				"cmd_local_sign_psbt_software",
				{
					request: {
						wallet_id: walletId,
						psbt_base64: psbtBase64,
						passphrase,
					},
				},
			);
			const broadcast = await invoke<LocalBroadcastPsbtResponse>(
				"cmd_local_broadcast_psbt",
				{
					request: {
						wallet_id: walletId,
						psbt_base64: signed.psbt_base64,
					},
				},
			);
			setTxid(broadcast.txid);
			setStep("done");
		} catch (err) {
			setError(typeof err === "string" ? err : "Sign or broadcast failed");
		} finally {
			setBusy(false);
		}
	};

	const copyTxid = async () => {
		if (!txid) return;
		try {
			await navigator.clipboard.writeText(txid);
			setCopiedTxid(true);
			setTimeout(() => setCopiedTxid(false), 1800);
		} catch {
			// clipboard unavailable
		}
	};

	const STEP_INDEX: Record<Step, number> = {
		compose: 0,
		confirm: 1,
		done: 2,
	};
	const stepIndex = STEP_INDEX[step];

	const sat = btcToSat(amountBtc);

	return (
		<div
			onMouseDown={onDrag}
			className="relative flex h-screen w-full select-none flex-col overflow-hidden bg-background"
		>
			<WindowControls />

			<div className="pointer-events-none absolute inset-0 bg-grid mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.10]" />

			<header
				className="relative flex shrink-0 items-center justify-between border-b border-border bg-card/60 px-8 pb-4 pt-10 backdrop-blur-sm"
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
							Send
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
				<div className="w-full max-w-[520px] space-y-8">
					<StepIndicator stepIndex={stepIndex} />

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

					{step === "compose" && (
						<ComposeStep
							recipient={recipient}
							onChangeRecipient={setRecipient}
							amountBtc={amountBtc}
							onChangeAmount={setAmountBtc}
							feeRate={feeRate}
							onChangeFeeRate={setFeeRate}
							balance={balance}
							sat={sat}
							busy={busy}
							onSubmit={buildPsbt}
						/>
					)}

					{step === "confirm" && (
						<ConfirmStep
							recipient={recipient}
							sat={sat ?? 0}
							feeRate={feeRate}
							passphrase={passphrase}
							onChangePassphrase={setPassphrase}
							busy={busy}
							onBack={() => {
								setStep("compose");
								setPsbtBase64(null);
								setPassphrase("");
								setError(null);
							}}
							onSubmit={signAndBroadcast}
						/>
					)}

					{step === "done" && txid && (
						<DoneStep
							txid={txid}
							copied={copiedTxid}
							onCopy={copyTxid}
							onFinish={() => navigate(`/local/wallets/${walletId}`)}
						/>
					)}
				</div>
			</div>
		</div>
	);
}

function StepIndicator({ stepIndex }: { stepIndex: number }) {
	const labels = ["Compose", "Confirm", "Done"];
	return (
		<div>
			<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
				§ — Send transaction
			</div>
			<div className="mt-3 flex items-center gap-2">
				{labels.map((label, i) => {
					const active = i === stepIndex;
					const done = i < stepIndex;
					return (
						<div key={label} className="flex flex-1 items-center gap-2">
							<div
								className={`h-1 flex-1 rounded-full transition-colors ${
									done
										? "bg-primary"
										: active
											? "bg-primary/60"
											: "bg-border"
								}`}
							/>
							<span
								className={`font-mono text-[9px] uppercase tracking-[0.18em] ${
									active || done
										? "text-foreground"
										: "text-muted-foreground/70"
								}`}
							>
								{label}
							</span>
						</div>
					);
				})}
			</div>
		</div>
	);
}

function ComposeStep({
	recipient,
	onChangeRecipient,
	amountBtc,
	onChangeAmount,
	feeRate,
	onChangeFeeRate,
	balance,
	sat,
	busy,
	onSubmit,
}: {
	recipient: string;
	onChangeRecipient: (v: string) => void;
	amountBtc: string;
	onChangeAmount: (v: string) => void;
	feeRate: string;
	onChangeFeeRate: (v: string) => void;
	balance: LocalBalance | null;
	sat: number | null;
	busy: boolean;
	onSubmit: () => void;
}) {
	const fr = Number(feeRate);
	const ready =
		recipient.trim().length > 0 &&
		sat !== null &&
		Number.isFinite(fr) &&
		fr > 0;
	const exceedsBalance =
		balance !== null && sat !== null && sat > balance.confirmed_sat;

	return (
		<form
			className="space-y-6"
			onSubmit={(e) => {
				e.preventDefault();
				if (ready && !busy) onSubmit();
			}}
		>
			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Recipient address
				</span>
				<input
					type="text"
					autoFocus
					value={recipient}
					onChange={(e) => onChangeRecipient(e.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck={false}
					placeholder="bcrt1q…"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[12px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Amount (BTC)
				</span>
				<input
					type="text"
					inputMode="decimal"
					value={amountBtc}
					onChange={(e) => onChangeAmount(e.target.value)}
					placeholder="0.001"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[14px] text-foreground outline-none transition-colors focus:border-primary"
				/>
				<div className="mt-1 flex items-center justify-between font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
					<span>{sat !== null ? `${sat.toLocaleString()} sat` : "—"}</span>
					{balance && (
						<span>
							balance {formatBtc(balance.confirmed_sat)} BTC
						</span>
					)}
				</div>
				{exceedsBalance && (
					<div className="mt-1 font-mono text-[10px] uppercase tracking-[0.14em] text-destructive">
						Amount exceeds confirmed balance
					</div>
				)}
			</label>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Fee rate (sat/vbyte)
				</span>
				<input
					type="text"
					inputMode="decimal"
					value={feeRate}
					onChange={(e) => onChangeFeeRate(e.target.value)}
					placeholder="2"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
				<p className="mt-1 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground/70">
					Rounded up to the nearest sat/vbyte
				</p>
			</label>

			<button
				type="submit"
				disabled={!ready || busy}
				className="flex h-11 w-full items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
			>
				{busy ? (
					<>
						<svg
							className="h-4 w-4 animate-spin"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2.5"
							strokeLinecap="round"
						>
							<path d="M21 12a9 9 0 1 1-6.219-8.56" />
						</svg>
						Building…
					</>
				) : (
					<>
						Continue
						<svg
							className="h-4 w-4"
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
					</>
				)}
			</button>
		</form>
	);
}

function ConfirmStep({
	recipient,
	sat,
	feeRate,
	passphrase,
	onChangePassphrase,
	busy,
	onBack,
	onSubmit,
}: {
	recipient: string;
	sat: number;
	feeRate: string;
	passphrase: string;
	onChangePassphrase: (v: string) => void;
	busy: boolean;
	onBack: () => void;
	onSubmit: () => void;
}) {
	return (
		<form
			className="space-y-6"
			onSubmit={(e) => {
				e.preventDefault();
				if (passphrase && !busy) onSubmit();
			}}
		>
			<div className="rounded-md border border-border bg-card px-4 py-4">
				<div className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Review
				</div>
				<dl className="mt-3 space-y-2 text-[12px]">
					<SummaryRow label="To">
						<span className="break-all font-mono text-foreground">
							{recipient}
						</span>
					</SummaryRow>
					<SummaryRow label="Amount">
						<span className="font-mono text-foreground">
							{formatBtc(sat)} BTC
						</span>
						<span className="ml-2 font-mono text-muted-foreground/80">
							{sat.toLocaleString()} sat
						</span>
					</SummaryRow>
					<SummaryRow label="Fee rate">
						<span className="font-mono text-foreground">
							{feeRate} sat/vB
						</span>
					</SummaryRow>
				</dl>
			</div>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Passphrase
				</span>
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					Re-enter the wallet passphrase to authorize signing. The seed is
					decrypted only for this transaction.
				</p>
				<input
					type="password"
					autoFocus
					value={passphrase}
					onChange={(e) => onChangePassphrase(e.target.value)}
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<div className="flex items-center gap-3">
				<button
					type="button"
					onClick={onBack}
					disabled={busy}
					className="flex h-11 items-center rounded-md border border-border bg-background px-5 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
				>
					← Back
				</button>
				<button
					type="submit"
					disabled={!passphrase || busy}
					className="flex h-11 flex-1 items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
				>
					{busy ? (
						<>
							<svg
								className="h-4 w-4 animate-spin"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2.5"
								strokeLinecap="round"
							>
								<path d="M21 12a9 9 0 1 1-6.219-8.56" />
							</svg>
							Signing & broadcasting…
						</>
					) : (
						"Sign & broadcast"
					)}
				</button>
			</div>
		</form>
	);
}

function SummaryRow({
	label,
	children,
}: {
	label: string;
	children: React.ReactNode;
}) {
	return (
		<div className="flex flex-col gap-0.5">
			<dt className="font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground/80">
				{label}
			</dt>
			<dd>{children}</dd>
		</div>
	);
}

function DoneStep({
	txid,
	copied,
	onCopy,
	onFinish,
}: {
	txid: string;
	copied: boolean;
	onCopy: () => void;
	onFinish: () => void;
}) {
	return (
		<div className="flex flex-col items-center text-center">
			<div className="flex h-14 w-14 items-center justify-center rounded-full border border-success/40 bg-success/[0.08]">
				<svg
					className="h-6 w-6 text-success"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2.5"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="M20 6 9 17l-5-5" />
				</svg>
			</div>
			<h2 className="mt-5 text-[18px] font-medium tracking-[-0.01em] text-foreground">
				Transaction broadcast.
			</h2>
			<p className="mt-2 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
				Your transaction is in the mempool. It'll appear in the wallet
				history once the next sync sees it.
			</p>

			<button
				type="button"
				onClick={onCopy}
				title="Click to copy"
				className="mt-6 w-full max-w-md rounded-md border border-border bg-card px-4 py-3 text-left font-mono text-[12px] text-foreground transition-colors hover:border-primary/60 hover:bg-primary/[0.03]"
			>
				<div className="flex items-center justify-between gap-3">
					<span className="break-all">{txid}</span>
					<span className="font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground">
						{copied ? "✓ Copied" : "Copy"}
					</span>
				</div>
			</button>

			<button
				type="button"
				onClick={onFinish}
				className="mt-8 flex h-11 items-center gap-2 rounded-md bg-primary px-6 text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px]"
			>
				Back to wallet
				<svg
					className="h-4 w-4"
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
			</button>
		</div>
	);
}
