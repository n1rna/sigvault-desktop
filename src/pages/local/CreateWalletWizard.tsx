// Singlesig wallet creation wizard (QBL-223).
//
// Two paths share the first three steps (basics → passphrase → review):
//   • Generate: backend mints a fresh BIP39 mnemonic, persists the
//     encrypted seed, and returns the words for one-time backup display.
//   • Recover: user types an existing 12/24-word phrase, backend
//     re-derives keys via `cmd_local_recover_from_mnemonic`.
//
// Multisig / Liana / watch-only flows ship in QBL-224 / 225 / 226.

import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import WindowControls from "../../components/WindowControls";
import type { LocalWalletCreateResponse } from "../../types/events";

type Method = "generate" | "recover";

type Step = "basics" | "passphrase" | "mnemonic" | "done";

interface Basics {
	name: string;
	network: string;
	method: Method;
}

const NETWORKS = [
	{ id: "regtest", label: "Regtest", hint: "Local dev / integration" },
	{ id: "signet", label: "Signet", hint: "Public test network" },
	{ id: "testnet4", label: "Testnet 4", hint: "Newer public testnet" },
];

export default function CreateWalletWizard() {
	const navigate = useNavigate();
	const [step, setStep] = useState<Step>("basics");
	const [basics, setBasics] = useState<Basics>({
		name: "",
		network: "regtest",
		method: "generate",
	});
	const [passphrase, setPassphrase] = useState("");
	const [confirmPassphrase, setConfirmPassphrase] = useState("");
	const [recoveryLength, setRecoveryLength] = useState<12 | 24>(24);
	const [recoveryWords, setRecoveryWords] = useState<string[]>(
		Array(24).fill(""),
	);
	const [generatedWords, setGeneratedWords] = useState<string[]>([]);
	const [createdId, setCreatedId] = useState<string | null>(null);
	const [backedUp, setBackedUp] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

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

	const cancel = () => navigate("/local/wallets");

	const submitCreate = async () => {
		setSubmitting(true);
		setError(null);
		try {
			let walletId: string;
			let words: string[] = [];
			if (basics.method === "generate") {
				const resp = await invoke<LocalWalletCreateResponse>(
					"cmd_local_create_wallet",
					{
						request: {
							name: basics.name,
							network: basics.network,
							policy_type: "singlesig_hot",
							passphrase,
						},
					},
				);
				walletId = resp.wallet_id;
				words = resp.mnemonic_words;
			} else {
				const mnemonic = recoveryWords
					.slice(0, recoveryLength)
					.map((w) => w.trim())
					.join(" ");
				walletId = await invoke<string>("cmd_local_recover_from_mnemonic", {
					request: {
						name: basics.name,
						network: basics.network,
						mnemonic,
						passphrase,
					},
				});
			}
			// Auto-unlock with the passphrase the user just typed so the
			// dashboard lands on a usable wallet (background sync kicks off
			// here too — see `cmd_local_unlock_wallet`).
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase },
			});
			setCreatedId(walletId);
			if (basics.method === "generate") {
				setGeneratedWords(words);
				setStep("mnemonic");
			} else {
				setStep("done");
			}
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to create wallet");
		} finally {
			setSubmitting(false);
		}
	};

	const finish = () => {
		if (!createdId) return;
		navigate(`/local/wallets/${createdId}`);
	};

	const STEP_INDEX: Record<Step, number> = {
		basics: 0,
		passphrase: 1,
		mnemonic: 2,
		done: 3,
	};
	const stepIndex = STEP_INDEX[step];

	return (
		<div
			onMouseDown={onDrag}
			className="relative flex h-screen w-full select-none overflow-hidden bg-background"
		>
			<WindowControls />

			<div className="pointer-events-none absolute inset-0 bg-grid mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.10]" />
			<div className="pointer-events-none absolute left-1/2 top-1/3 h-[420px] w-[420px] -translate-x-1/2 rounded-full bg-primary/[0.06] blur-[140px]" />

			<main className="relative flex flex-1 items-start justify-center overflow-y-auto px-6 py-10">
				<div
					className="relative w-full max-w-[520px]"
					onMouseDown={(e) => e.stopPropagation()}
				>
					<Header
						stepIndex={stepIndex}
						onCancel={cancel}
						method={basics.method}
					/>

					{error && (
						<div className="mt-6 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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

					{step === "basics" && (
						<BasicsStep
							value={basics}
							onChange={setBasics}
							onNext={() => {
								setError(null);
								setStep("passphrase");
							}}
						/>
					)}

					{step === "passphrase" && (
						<PassphraseStep
							passphrase={passphrase}
							confirmPassphrase={confirmPassphrase}
							onChangePassphrase={setPassphrase}
							onChangeConfirm={setConfirmPassphrase}
							method={basics.method}
							recoveryLength={recoveryLength}
							onChangeRecoveryLength={setRecoveryLength}
							recoveryWords={recoveryWords}
							onChangeRecoveryWord={(i, v) =>
								setRecoveryWords((prev) => {
									const next = [...prev];
									next[i] = v;
									return next;
								})
							}
							onBack={() => setStep("basics")}
							onSubmit={submitCreate}
							submitting={submitting}
						/>
					)}

					{step === "mnemonic" && basics.method === "generate" && (
						<MnemonicDisplayStep
							words={generatedWords}
							backedUp={backedUp}
							onToggleBackedUp={setBackedUp}
							onContinue={() => setStep("done")}
						/>
					)}

					{step === "done" && (
						<DoneStep method={basics.method} onContinue={finish} />
					)}
				</div>
			</main>
		</div>
	);
}

function Header({
	stepIndex,
	onCancel,
	method,
}: {
	stepIndex: number;
	onCancel: () => void;
	method: Method;
}) {
	const labels =
		method === "generate"
			? ["Basics", "Passphrase", "Backup", "Done"]
			: ["Basics", "Recovery", "Done", ""];
	return (
		<div>
			<div className="flex items-center justify-between">
				<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ — New wallet
				</div>
				<button
					type="button"
					onClick={onCancel}
					className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground"
				>
					Cancel
				</button>
			</div>
			<h1 className="mt-3 text-[26px] font-medium leading-[1.15] tracking-[-0.015em] text-foreground">
				Create a singlesig wallet.
			</h1>

			<div className="mt-7 flex items-center gap-2">
				{labels
					.filter((l) => l)
					.map((label, i) => {
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

function BasicsStep({
	value,
	onChange,
	onNext,
}: {
	value: Basics;
	onChange: (v: Basics) => void;
	onNext: () => void;
}) {
	const ready = value.name.trim().length > 0;
	return (
		<form
			className="mt-8 space-y-6"
			onSubmit={(e) => {
				e.preventDefault();
				if (ready) onNext();
			}}
		>
			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Wallet name
				</span>
				<input
					type="text"
					autoFocus
					value={value.name}
					onChange={(e) => onChange({ ...value, name: e.target.value })}
					placeholder="My local wallet"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<div>
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Network
				</span>
				<div className="mt-2 grid grid-cols-3 gap-2">
					{NETWORKS.map((n) => {
						const selected = value.network === n.id;
						return (
							<button
								key={n.id}
								type="button"
								onClick={() => onChange({ ...value, network: n.id })}
								className={`flex flex-col items-start rounded-md border px-3 py-2.5 text-left transition-colors ${
									selected
										? "border-primary bg-primary/[0.06]"
										: "border-border bg-card hover:border-primary/60"
								}`}
							>
								<span className="text-[12px] font-medium text-foreground">
									{n.label}
								</span>
								<span className="mt-0.5 font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground">
									{n.hint}
								</span>
							</button>
						);
					})}
				</div>
				<p className="mt-2 font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground/70">
					Mainnet not available in v1
				</p>
			</div>

			<div>
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Method
				</span>
				<div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-2">
					<MethodCard
						active={value.method === "generate"}
						onClick={() => onChange({ ...value, method: "generate" })}
						title="Generate new"
						hint="Create a fresh BIP39 mnemonic on this device."
					/>
					<MethodCard
						active={value.method === "recover"}
						onClick={() => onChange({ ...value, method: "recover" })}
						title="Recover"
						hint="Restore from an existing 12-word phrase."
					/>
				</div>
			</div>

			<button
				type="submit"
				disabled={!ready}
				className="flex h-11 w-full items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
			>
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
			</button>
		</form>
	);
}

function MethodCard({
	active,
	onClick,
	title,
	hint,
}: {
	active: boolean;
	onClick: () => void;
	title: string;
	hint: string;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={`flex flex-col items-start rounded-md border p-3 text-left transition-colors ${
				active
					? "border-primary bg-primary/[0.06]"
					: "border-border bg-card hover:border-primary/60"
			}`}
		>
			<span className="text-[13px] font-medium text-foreground">{title}</span>
			<span className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
				{hint}
			</span>
		</button>
	);
}

function PassphraseStep({
	passphrase,
	confirmPassphrase,
	onChangePassphrase,
	onChangeConfirm,
	method,
	recoveryLength,
	onChangeRecoveryLength,
	recoveryWords,
	onChangeRecoveryWord,
	onBack,
	onSubmit,
	submitting,
}: {
	passphrase: string;
	confirmPassphrase: string;
	onChangePassphrase: (v: string) => void;
	onChangeConfirm: (v: string) => void;
	method: Method;
	recoveryLength: 12 | 24;
	onChangeRecoveryLength: (n: 12 | 24) => void;
	recoveryWords: string[];
	onChangeRecoveryWord: (i: number, v: string) => void;
	onBack: () => void;
	onSubmit: () => void;
	submitting: boolean;
}) {
	const passphraseOk = passphrase.length >= 8 && passphrase === confirmPassphrase;
	const recoveryOk =
		method === "generate" ||
		recoveryWords
			.slice(0, recoveryLength)
			.every((w) => w.trim().length > 0);
	const ready = passphraseOk && recoveryOk;

	return (
		<form
			className="mt-8 space-y-6"
			onSubmit={(e) => {
				e.preventDefault();
				if (ready) onSubmit();
			}}
		>
			{method === "recover" && (
				<div>
					<div className="flex items-center justify-between">
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							Recovery phrase
						</span>
						<div className="flex items-center gap-1 rounded-md border border-border bg-card p-0.5">
							{([12, 24] as const).map((n) => (
								<button
									key={n}
									type="button"
									onClick={() => onChangeRecoveryLength(n)}
									className={`rounded-sm px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.16em] transition-colors ${
										recoveryLength === n
											? "bg-primary/[0.10] text-foreground"
											: "text-muted-foreground hover:text-foreground"
									}`}
								>
									{n} words
								</button>
							))}
						</div>
					</div>
					<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
						Enter your existing BIP39 phrase, lowercase, in order.
					</p>
					<div className="mt-3 grid grid-cols-3 gap-2">
						{recoveryWords.slice(0, recoveryLength).map((w, i) => (
							<div
								key={i}
								className="flex items-center gap-2 rounded-md border border-border bg-card px-2.5 py-1.5"
							>
								<span className="font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground/70">
									{String(i + 1).padStart(2, "0")}
								</span>
								<input
									type="text"
									value={w}
									onChange={(e) =>
										onChangeRecoveryWord(i, e.target.value.toLowerCase())
									}
									autoComplete="off"
									autoCapitalize="off"
									spellCheck={false}
									className="flex-1 bg-transparent font-mono text-[12px] text-foreground outline-none"
								/>
							</div>
						))}
					</div>
				</div>
			)}

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Passphrase
				</span>
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					Encrypts the seed on disk. You'll need this to unlock the wallet.
					At least 8 characters.
				</p>
				<input
					type="password"
					autoFocus={method === "generate"}
					value={passphrase}
					onChange={(e) => onChangePassphrase(e.target.value)}
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Confirm passphrase
				</span>
				<input
					type="password"
					value={confirmPassphrase}
					onChange={(e) => onChangeConfirm(e.target.value)}
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
				{confirmPassphrase.length > 0 && passphrase !== confirmPassphrase && (
					<span className="mt-1 block font-mono text-[10px] uppercase tracking-[0.14em] text-destructive">
						Passphrases do not match
					</span>
				)}
			</label>

			<div className="flex items-center gap-3">
				<button
					type="button"
					onClick={onBack}
					disabled={submitting}
					className="flex h-11 items-center rounded-md border border-border bg-background px-5 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
				>
					← Back
				</button>
				<button
					type="submit"
					disabled={!ready || submitting}
					className="flex h-11 flex-1 items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
				>
					{submitting ? (
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
							{method === "generate" ? "Generating…" : "Recovering…"}
						</>
					) : method === "generate" ? (
						"Generate wallet"
					) : (
						"Recover wallet"
					)}
				</button>
			</div>
		</form>
	);
}

function MnemonicDisplayStep({
	words,
	backedUp,
	onToggleBackedUp,
	onContinue,
}: {
	words: string[];
	backedUp: boolean;
	onToggleBackedUp: (v: boolean) => void;
	onContinue: () => void;
}) {
	const wordsString = useMemo(() => words.join(" "), [words]);
	const [copied, setCopied] = useState(false);

	const copy = async () => {
		try {
			await navigator.clipboard.writeText(wordsString);
			setCopied(true);
			setTimeout(() => setCopied(false), 1800);
		} catch {
			// clipboard may be unavailable; user can still write the words by hand.
		}
	};

	return (
		<div className="mt-8 space-y-6">
			<div className="rounded-md border border-warning/40 bg-warning/[0.06] px-4 py-3">
				<div className="flex items-start gap-2.5">
					<svg
						className="mt-0.5 h-4 w-4 shrink-0 text-warning"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
						<line x1="12" y1="9" x2="12" y2="13" />
						<line x1="12" y1="17" x2="12.01" y2="17" />
					</svg>
					<div className="text-[12px] leading-relaxed text-foreground">
						<p className="font-medium">
							Record this phrase. It is the only way to recover your wallet
							if you forget your passphrase.
						</p>
						<p className="mt-1 text-muted-foreground">
							Write it down on paper. Do not screenshot. Anyone with these
							words can spend your bitcoin.
						</p>
					</div>
				</div>
			</div>

			<div className="grid grid-cols-3 gap-2">
				{words.map((w, i) => (
					<div
						key={i}
						className="flex items-center gap-2 rounded-md border border-border bg-card px-2.5 py-2"
					>
						<span className="font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground/70">
							{String(i + 1).padStart(2, "0")}
						</span>
						<span className="flex-1 font-mono text-[12px] text-foreground">
							{w}
						</span>
					</div>
				))}
			</div>

			<button
				type="button"
				onClick={copy}
				className="w-full font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground"
			>
				{copied ? "✓ Copied" : "Copy to clipboard"}
			</button>

			<label className="flex cursor-pointer items-start gap-3 rounded-md border border-border bg-card px-4 py-3">
				<input
					type="checkbox"
					checked={backedUp}
					onChange={(e) => onToggleBackedUp(e.target.checked)}
					className="mt-0.5 h-4 w-4 cursor-pointer accent-primary"
				/>
				<span className="text-[12px] leading-relaxed text-foreground">
					I have recorded my recovery phrase in a safe place. I understand
					this is the only way to restore the wallet if I lose my passphrase.
				</span>
			</label>

			<button
				type="button"
				onClick={onContinue}
				disabled={!backedUp}
				className="flex h-11 w-full items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
			>
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
			</button>
		</div>
	);
}

function DoneStep({
	method,
	onContinue,
}: {
	method: Method;
	onContinue: () => void;
}) {
	return (
		<div className="mt-12 flex flex-col items-center text-center">
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
				{method === "generate"
					? "Wallet created."
					: "Wallet recovered."}
			</h2>
			<p className="mt-2 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
				The wallet is unlocked and ready. Open it to view balance, addresses,
				and history.
			</p>
			<button
				type="button"
				onClick={onContinue}
				className="mt-8 flex h-11 items-center gap-2 rounded-md bg-primary px-6 text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px]"
			>
				Open wallet
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
