// Wallet recovery wizard (QBL-230).
//
// Three flavours, picked on the first step:
//   1. Singlesig — restore a hot singlesig wallet from a BIP39 phrase.
//      Backend = `cmd_local_recover_from_mnemonic`. Same code path the
//      original creation wizard offers, just surfaced as a dedicated
//      entry point so users who want to recover can find it.
//   2. Cosigner (multisig / Liana) — restore a wallet where one slot
//      is a hot key. The user pastes the wallet's descriptor pair and
//      we derive the matching slot from the supplied mnemonic. Backend
//      verifies the master fingerprint actually appears in the
//      descriptor before persisting anything.
//   3. Descriptor only — for read-only / HW-spend imports. Routes to
//      the existing watch-only flow in the create wizard.

import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { validateMnemonic } from "@scure/bip39";
import { wordlist as bip39Wordlist } from "@scure/bip39/wordlists/english.js";
import WindowControls from "../../components/WindowControls";

type Method = "singlesig" | "cosigner" | "descriptor_only";
type Step = "method" | "details" | "done";

const NETWORKS = ["regtest", "testnet", "signet"] as const;
type Network = (typeof NETWORKS)[number];

const RECOVERY_LENGTHS = [12, 15, 18, 21, 24] as const;
type RecoveryLength = (typeof RECOVERY_LENGTHS)[number];

export default function RecoveryFlow() {
	const navigate = useNavigate();
	const [step, setStep] = useState<Step>("method");
	const [method, setMethod] = useState<Method>("singlesig");
	const [name, setName] = useState("");
	const [network, setNetwork] = useState<Network>("regtest");
	const [recoveryLength, setRecoveryLength] = useState<RecoveryLength>(24);
	const [words, setWords] = useState<string[]>(() => Array(24).fill(""));
	const [externalDescriptor, setExternalDescriptor] = useState("");
	const [internalDescriptor, setInternalDescriptor] = useState("");
	const [policyType, setPolicyType] = useState<"multisig" | "liana">("multisig");
	const [bip39Passphrase, setBip39Passphrase] = useState("");
	const [passphrase, setPassphrase] = useState("");
	const [confirmPassphrase, setConfirmPassphrase] = useState("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [createdId, setCreatedId] = useState<string | null>(null);

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

	const mnemonic = useMemo(
		() =>
			words
				.slice(0, recoveryLength)
				.map((w) => w.trim().toLowerCase())
				.join(" "),
		[words, recoveryLength],
	);

	const allWordsFilled = useMemo(
		() => words.slice(0, recoveryLength).every((w) => w.trim().length > 0),
		[words, recoveryLength],
	);
	// Live BIP39 checksum check: `validateMnemonic` returns true only
	// when every word is in the English wordlist AND the last word
	// encodes a valid checksum over the first N-1. Surfaces typos at
	// the input step instead of failing at submit time on the backend.
	const mnemonicValid = useMemo(() => {
		if (!allWordsFilled) return false;
		try {
			return validateMnemonic(mnemonic, bip39Wordlist);
		} catch {
			return false;
		}
	}, [mnemonic, allWordsFilled]);
	const passphraseOk = passphrase.length >= 8 && passphrase === confirmPassphrase;
	const nameOk = name.trim().length > 0;

	const detailsReady =
		nameOk &&
		passphraseOk &&
		mnemonicValid &&
		(method === "singlesig" ||
			(externalDescriptor.trim().length > 0 &&
				internalDescriptor.trim().length > 0));

	const submit = async () => {
		setSubmitting(true);
		setError(null);
		try {
			let walletId: string;
			if (method === "singlesig") {
				walletId = await invoke<string>("cmd_local_recover_from_mnemonic", {
					request: {
						name: name.trim(),
						network,
						mnemonic,
						passphrase,
						bip39_passphrase: bip39Passphrase,
					},
				});
			} else {
				walletId = await invoke<string>("cmd_local_recover_cosigner", {
					request: {
						name: name.trim(),
						network,
						mnemonic,
						bip39_passphrase: bip39Passphrase,
						encrypt_passphrase: passphrase,
						external_descriptor: externalDescriptor.trim(),
						internal_descriptor: internalDescriptor.trim(),
						policy_type: policyType,
					},
				});
			}
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase },
			});
			setCreatedId(walletId);
			setStep("done");
		} catch (err) {
			setError(
				typeof err === "string"
					? err
					: err instanceof Error
						? err.message
						: "Recovery failed",
			);
		} finally {
			setSubmitting(false);
		}
	};

	const stepIndex = step === "method" ? 0 : step === "details" ? 1 : 2;

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
						onClick={() => navigate("/local/wallets")}
						className="flex h-8 w-8 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
						title="Back to wallet list"
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
							Recover wallet
						</span>
						<span className="mt-1 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
							Restore from a backup
						</span>
					</div>
				</div>
			</header>

			<div
				className="relative flex flex-1 items-start justify-center overflow-y-auto px-6 py-10"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="w-full max-w-[640px] space-y-8">
					<StepIndicator stepIndex={stepIndex} />

					{error && (
						<div className="rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
							{error}
						</div>
					)}

					{step === "method" && (
						<MethodStep
							value={method}
							onChange={setMethod}
							onNext={() => {
								if (method === "descriptor_only") {
									navigate("/local/wallets/new?method=watch_only");
									return;
								}
								setStep("details");
							}}
						/>
					)}

					{step === "details" && (
						<DetailsStep
							method={method}
							name={name}
							onChangeName={setName}
							network={network}
							onChangeNetwork={setNetwork}
							recoveryLength={recoveryLength}
							onChangeRecoveryLength={setRecoveryLength}
							words={words}
							onChangeWord={(i, v) => {
								setWords((prev) => {
									const copy = [...prev];
									copy[i] = v;
									return copy;
								});
							}}
							mnemonicValid={mnemonicValid}
							allWordsFilled={allWordsFilled}
							externalDescriptor={externalDescriptor}
							onChangeExternalDescriptor={setExternalDescriptor}
							internalDescriptor={internalDescriptor}
							onChangeInternalDescriptor={setInternalDescriptor}
							policyType={policyType}
							onChangePolicyType={setPolicyType}
							bip39Passphrase={bip39Passphrase}
							onChangeBip39Passphrase={setBip39Passphrase}
							passphrase={passphrase}
							onChangePassphrase={setPassphrase}
							confirmPassphrase={confirmPassphrase}
							onChangeConfirm={setConfirmPassphrase}
							submitting={submitting}
							ready={detailsReady}
							onBack={() => setStep("method")}
							onSubmit={submit}
						/>
					)}

					{step === "done" && createdId && (
						<DoneStep
							walletId={createdId}
							onFinish={() =>
								navigate(`/local/wallets/${createdId}`)
							}
						/>
					)}
				</div>
			</div>
		</div>
	);
}

function StepIndicator({ stepIndex }: { stepIndex: number }) {
	const labels = ["Method", "Details", "Done"];
	return (
		<div>
			<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
				§ — Recover wallet
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

function MethodStep({
	value,
	onChange,
	onNext,
}: {
	value: Method;
	onChange: (m: Method) => void;
	onNext: () => void;
}) {
	const options: { id: Method; title: string; hint: string }[] = [
		{
			id: "singlesig",
			title: "Singlesig (BIP39 seed)",
			hint: "Restore a hot singlesig wallet from its 12 / 15 / 18 / 21 / 24-word phrase.",
		},
		{
			id: "cosigner",
			title: "Multisig / Liana hot cosigner",
			hint: "Restore a wallet where one slot is a hot key. Needs the wallet's descriptor pair + your mnemonic. We'll match your slot by master fingerprint.",
		},
		{
			id: "descriptor_only",
			title: "Descriptor only (read-only or HW spend)",
			hint: "Import a wallet by pasting its descriptor. No seed needed. Routes you to the watch-only flow which can also be spend-enabled via a hardware wallet.",
		},
	];
	return (
		<div className="space-y-5">
			<div className="space-y-2">
				{options.map((opt) => {
					const active = opt.id === value;
					return (
						<button
							key={opt.id}
							type="button"
							onClick={() => onChange(opt.id)}
							className={`block w-full rounded-md border px-4 py-3 text-left transition-colors ${
								active
									? "border-primary bg-primary/[0.06]"
									: "border-border bg-card/40 hover:border-primary/50"
							}`}
						>
							<div className="text-[13px] font-medium text-foreground">
								{opt.title}
							</div>
							<p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">
								{opt.hint}
							</p>
						</button>
					);
				})}
			</div>
			<button
				type="button"
				onClick={onNext}
				className="flex h-11 w-full items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px]"
			>
				Continue
			</button>
		</div>
	);
}

function DetailsStep({
	method,
	name,
	onChangeName,
	network,
	onChangeNetwork,
	recoveryLength,
	onChangeRecoveryLength,
	words,
	onChangeWord,
	mnemonicValid,
	allWordsFilled,
	externalDescriptor,
	onChangeExternalDescriptor,
	internalDescriptor,
	onChangeInternalDescriptor,
	policyType,
	onChangePolicyType,
	bip39Passphrase,
	onChangeBip39Passphrase,
	passphrase,
	onChangePassphrase,
	confirmPassphrase,
	onChangeConfirm,
	submitting,
	ready,
	onBack,
	onSubmit,
}: {
	method: Method;
	name: string;
	onChangeName: (v: string) => void;
	network: Network;
	onChangeNetwork: (n: Network) => void;
	recoveryLength: RecoveryLength;
	onChangeRecoveryLength: (n: RecoveryLength) => void;
	words: string[];
	onChangeWord: (i: number, v: string) => void;
	mnemonicValid: boolean;
	allWordsFilled: boolean;
	externalDescriptor: string;
	onChangeExternalDescriptor: (v: string) => void;
	internalDescriptor: string;
	onChangeInternalDescriptor: (v: string) => void;
	policyType: "multisig" | "liana";
	onChangePolicyType: (p: "multisig" | "liana") => void;
	bip39Passphrase: string;
	onChangeBip39Passphrase: (v: string) => void;
	passphrase: string;
	onChangePassphrase: (v: string) => void;
	confirmPassphrase: string;
	onChangeConfirm: (v: string) => void;
	submitting: boolean;
	ready: boolean;
	onBack: () => void;
	onSubmit: () => void;
}) {
	return (
		<form
			className="space-y-6"
			onSubmit={(e) => {
				e.preventDefault();
				if (ready && !submitting) onSubmit();
			}}
		>
			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Wallet name
				</span>
				<input
					type="text"
					value={name}
					onChange={(e) => onChangeName(e.target.value)}
					placeholder="Recovered savings vault"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<div>
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Network
				</span>
				<div className="mt-2 grid grid-cols-3 gap-2 rounded-md border border-border bg-card/40 p-1">
					{NETWORKS.map((n) => (
						<button
							key={n}
							type="button"
							onClick={() => onChangeNetwork(n)}
							className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
								network === n
									? "bg-primary text-primary-foreground"
									: "text-muted-foreground hover:text-foreground"
							}`}
						>
							{n}
						</button>
					))}
				</div>
			</div>

			<div>
				<div className="flex items-center justify-between">
					<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
						Recovery phrase
					</span>
					<div className="flex items-center gap-2">
						<ChecksumStatus
							allFilled={allWordsFilled}
							valid={mnemonicValid}
						/>
						<div className="flex items-center gap-1 rounded-md border border-border bg-card p-0.5">
							{RECOVERY_LENGTHS.map((n) => (
								<button
									key={n}
									type="button"
									onClick={() => onChangeRecoveryLength(n)}
									className={`rounded-sm px-2 py-1 font-mono text-[9px] uppercase tracking-[0.14em] transition-colors ${
										recoveryLength === n
											? "bg-primary/[0.10] text-foreground"
											: "text-muted-foreground hover:text-foreground"
									}`}
								>
									{n}
								</button>
							))}
						</div>
					</div>
				</div>
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					Enter your existing BIP39 phrase, lowercase, in order. Use Tab or
					↓/↑ to pick from the autocomplete suggestions.
				</p>
				<div className="mt-3 grid grid-cols-3 gap-2">
					{words.slice(0, recoveryLength).map((w, i) => (
						<WordInput
							key={i}
							index={i}
							value={w}
							onChange={(v) => onChangeWord(i, v)}
						/>
					))}
				</div>
			</div>

			{method === "cosigner" && (
				<>
					<div>
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							Policy type
						</span>
						<div className="mt-2 grid grid-cols-2 gap-2 rounded-md border border-border bg-card/40 p-1">
							<button
								type="button"
								onClick={() => onChangePolicyType("multisig")}
								className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
									policyType === "multisig"
										? "bg-primary text-primary-foreground"
										: "text-muted-foreground hover:text-foreground"
								}`}
							>
								Multisig
							</button>
							<button
								type="button"
								onClick={() => onChangePolicyType("liana")}
								className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
									policyType === "liana"
										? "bg-primary text-primary-foreground"
										: "text-muted-foreground hover:text-foreground"
								}`}
							>
								Liana
							</button>
						</div>
					</div>
					<label className="block">
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							External descriptor (receive)
						</span>
						<textarea
							value={externalDescriptor}
							onChange={(e) => onChangeExternalDescriptor(e.target.value)}
							autoComplete="off"
							spellCheck={false}
							rows={3}
							placeholder="wsh(sortedmulti(2,[fp1/48'/1'/0'/2']xpub.../0/*,...))"
							className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
						/>
					</label>
					<label className="block">
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							Internal descriptor (change)
						</span>
						<textarea
							value={internalDescriptor}
							onChange={(e) => onChangeInternalDescriptor(e.target.value)}
							autoComplete="off"
							spellCheck={false}
							rows={3}
							placeholder="wsh(sortedmulti(2,[fp1/48'/1'/0'/2']xpub.../1/*,...))"
							className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
						/>
					</label>
				</>
			)}

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					BIP39 passphrase (optional)
				</span>
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					The "25th word" some users layer on top of their seed phrase.
					Leave empty unless you actively used one when the wallet was
					created — the cryptographic seed differs between empty and
					non-empty values, so a wrong guess produces a different wallet.
				</p>
				<input
					type="password"
					value={bip39Passphrase}
					onChange={(e) => onChangeBip39Passphrase(e.target.value)}
					autoComplete="off"
					spellCheck={false}
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[12px] text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Wallet-encryption passphrase
				</span>
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					Encrypts the seed on disk. You'll need this to unlock the wallet.
					At least 8 characters.
				</p>
				<input
					type="password"
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
				{confirmPassphrase.length > 0 &&
					confirmPassphrase !== passphrase && (
						<span className="mt-1 block font-mono text-[10px] uppercase tracking-[0.14em] text-destructive">
							Passphrases don't match
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
					{submitting ? "Recovering…" : "Recover wallet"}
				</button>
			</div>
		</form>
	);
}

function ChecksumStatus({
	allFilled,
	valid,
}: {
	allFilled: boolean;
	valid: boolean;
}) {
	if (!allFilled) {
		return (
			<span className="rounded-sm border border-border bg-card px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground">
				Incomplete
			</span>
		);
	}
	return valid ? (
		<span className="rounded-sm border border-emerald-500/40 bg-emerald-500/[0.08] px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.14em] text-emerald-600 dark:text-emerald-400">
			Valid checksum
		</span>
	) : (
		<span className="rounded-sm border border-destructive/40 bg-destructive/[0.06] px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.14em] text-destructive">
			Bad checksum
		</span>
	);
}

// Per-word input with prefix-match autocomplete against the BIP39
// English wordlist. We bound the suggestion list to 5 entries and only
// show it while the input is focused and the typed prefix is at least
// 2 characters — keeps the UI calm during fast typing while still
// catching typos as the user moves between cells.
function WordInput({
	index,
	value,
	onChange,
}: {
	index: number;
	value: string;
	onChange: (v: string) => void;
}) {
	const [focused, setFocused] = useState(false);
	const [highlight, setHighlight] = useState(0);
	const suggestions = useMemo(() => {
		const prefix = value.trim().toLowerCase();
		if (prefix.length < 2) return [];
		// `bip39Wordlist` is alphabetically sorted; a binary-search
		// scan would be tighter but at 2048 entries the linear filter
		// is imperceptible and keeps the code easy to read.
		const matches: string[] = [];
		for (const w of bip39Wordlist) {
			if (w.startsWith(prefix)) {
				matches.push(w);
				if (matches.length >= 5) break;
			}
		}
		return matches;
	}, [value]);

	const showSuggestions =
		focused && suggestions.length > 0 && !suggestions.includes(value);

	const accept = (word: string) => {
		onChange(word);
		setFocused(false);
	};

	return (
		<div className="relative">
			<div
				className={`flex items-center gap-2 rounded-md border bg-card px-2.5 py-1.5 ${
					value && !bip39Wordlist.includes(value)
						? "border-destructive/50"
						: "border-border"
				}`}
			>
				<span className="font-mono text-[9px] uppercase tracking-[0.14em] text-muted-foreground/70">
					{String(index + 1).padStart(2, "0")}
				</span>
				<input
					type="text"
					value={value}
					onChange={(e) => {
						onChange(e.target.value.toLowerCase());
						setHighlight(0);
					}}
					onFocus={() => setFocused(true)}
					onBlur={() =>
						// Delay so an onMouseDown on a suggestion has a chance
						// to fire before we hide the list.
						setTimeout(() => setFocused(false), 100)
					}
					onKeyDown={(e) => {
						if (!showSuggestions) return;
						if (e.key === "ArrowDown") {
							e.preventDefault();
							setHighlight((h) => Math.min(h + 1, suggestions.length - 1));
						} else if (e.key === "ArrowUp") {
							e.preventDefault();
							setHighlight((h) => Math.max(h - 1, 0));
						} else if (e.key === "Enter" || e.key === "Tab") {
							e.preventDefault();
							accept(suggestions[highlight] ?? suggestions[0]);
						}
					}}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck={false}
					className="flex-1 bg-transparent font-mono text-[12px] text-foreground outline-none"
				/>
			</div>
			{showSuggestions && (
				<div className="absolute left-0 right-0 top-full z-10 mt-1 overflow-hidden rounded-md border border-border bg-card shadow-lg">
					{suggestions.map((s, i) => (
						<button
							key={s}
							type="button"
							onMouseDown={(e) => {
								e.preventDefault();
								accept(s);
							}}
							className={`flex w-full items-center px-2.5 py-1 font-mono text-[11px] ${
								i === highlight
									? "bg-primary/[0.10] text-foreground"
									: "text-muted-foreground hover:bg-muted/40 hover:text-foreground"
							}`}
						>
							{s}
						</button>
					))}
				</div>
			)}
		</div>
	);
}

function DoneStep({
	walletId,
	onFinish,
}: {
	walletId: string;
	onFinish: () => void;
}) {
	return (
		<div className="space-y-6">
			<div className="rounded-md border border-border bg-card/40 px-5 py-6 text-center">
				<div className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Wallet recovered
				</div>
				<div className="mt-2 font-mono text-[12px] text-foreground">
					{walletId.slice(0, 12)}…{walletId.slice(-4)}
				</div>
				<p className="mt-3 text-[12px] leading-relaxed text-muted-foreground">
					The wallet is unlocked and syncing. Balance + history will populate
					as the sync completes.
				</p>
			</div>
			<button
				type="button"
				onClick={onFinish}
				className="flex h-11 w-full items-center justify-center gap-2 rounded-md bg-primary text-[13px] font-medium text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px]"
			>
				Open wallet
			</button>
		</div>
	);
}
