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
	const passphraseOk = passphrase.length >= 8 && passphrase === confirmPassphrase;
	const nameOk = name.trim().length > 0;

	const detailsReady =
		nameOk &&
		passphraseOk &&
		allWordsFilled &&
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
					},
				});
			} else {
				walletId = await invoke<string>("cmd_local_recover_cosigner", {
					request: {
						name: name.trim(),
						network,
						mnemonic,
						bip39_passphrase: "",
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
							externalDescriptor={externalDescriptor}
							onChangeExternalDescriptor={setExternalDescriptor}
							internalDescriptor={internalDescriptor}
							onChangeInternalDescriptor={setInternalDescriptor}
							policyType={policyType}
							onChangePolicyType={setPolicyType}
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
	externalDescriptor,
	onChangeExternalDescriptor,
	internalDescriptor,
	onChangeInternalDescriptor,
	policyType,
	onChangePolicyType,
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
	externalDescriptor: string;
	onChangeExternalDescriptor: (v: string) => void;
	internalDescriptor: string;
	onChangeInternalDescriptor: (v: string) => void;
	policyType: "multisig" | "liana";
	onChangePolicyType: (p: "multisig" | "liana") => void;
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
				<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
					Enter your existing BIP39 phrase, lowercase, in order.
				</p>
				<div className="mt-3 grid grid-cols-3 gap-2">
					{words.slice(0, recoveryLength).map((w, i) => (
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
									onChangeWord(i, e.target.value.toLowerCase())
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
