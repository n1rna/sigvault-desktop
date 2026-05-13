// Wallet creation wizard. One screen, several methods sharing the basics
// step:
//   • Generate (hot): backend mints a fresh BIP39 mnemonic, persists
//     the encrypted seed, returns the words for one-time backup display.
//   • Recover (hot): user types an existing 12/24-word phrase, backend
//     re-derives keys via `cmd_local_recover_from_mnemonic`.
//   • Hardware: user connects a Ledger / Trezor / BitBox / Coldcard,
//     unlocks it, the wizard collects the device's xpub at the standard
//     singlesig segwit-v0 path, and persists a watch-only descriptor
//     via `cmd_local_create_singlesig_hw`. No on-disk seed.
//   • Watch-only: paste descriptors directly (QBL-226).
//   • Multisig: M-of-N from pasted cosigner xpubs (QBL-224).
//   • Liana: timelocked-policy with primary + recovery paths, all
//     pasted xpubs (QBL-225). Hot primary keys are out of scope for v1
//     — see QBL-235 for the related "unspendable primary" affordance.

import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DeviceDiscovery from "../../components/DeviceDiscovery";
import WindowControls from "../../components/WindowControls";
import { SUPPORTED_NETWORKS } from "../../constants/networks";
import type { LocalWalletCreateResponse } from "../../types/events";
import type { DeviceInfo } from "../../types/hardware";

type Method =
	| "generate"
	| "recover"
	| "hardware"
	| "watch_only"
	| "multisig"
	| "liana";

type Step =
	| "basics"
	| "passphrase"
	| "hw"
	| "watch_only"
	| "multisig"
	| "liana"
	| "mnemonic"
	| "done";

/** BIP44 coin index by network — mirrors policy-core's
 * `KeyUtils::get_primary_derivation_path`. Mainnet uses coin 0, every
 * other supported network uses coin 1. */
function primaryDerivationPath(network: string): string {
	const coin = network === "bitcoin" ? "0'" : "1'";
	return `m/84'/${coin}/0'`;
}

interface Basics {
	name: string;
	network: string;
	method: Method;
}

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

	const submitMultisig = async (input: {
		threshold: number;
		cosigners: { key: string; fingerprint?: string }[];
	}) => {
		setSubmitting(true);
		setError(null);
		try {
			const walletId = await invoke<string>("cmd_local_create_multisig", {
				request: {
					name: basics.name,
					network: basics.network,
					threshold: input.threshold,
					cosigners: input.cosigners,
				},
			});
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase: "" },
			});
			setCreatedId(walletId);
			setStep("done");
		} catch (err) {
			setError(
				typeof err === "string" ? err : "Failed to create multisig wallet",
			);
		} finally {
			setSubmitting(false);
		}
	};

	const submitWatchOnly = async (descriptors: {
		external: string;
		internal: string;
		fingerprints: string[];
		spendable: boolean;
	}) => {
		setSubmitting(true);
		setError(null);
		try {
			const walletId = await invoke<string>("cmd_local_create_watch_only", {
				request: {
					name: basics.name,
					network: basics.network,
					external_descriptor: descriptors.external,
					internal_descriptor: descriptors.internal,
					fingerprints: descriptors.fingerprints,
					spendable: descriptors.spendable,
				},
			});
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase: "" },
			});
			setCreatedId(walletId);
			setStep("done");
		} catch (err) {
			setError(
				typeof err === "string" ? err : "Failed to create watch-only wallet",
			);
		} finally {
			setSubmitting(false);
		}
	};

	const submitLiana = async (input: {
		primary:
			| { kind: "keys"; key: { fingerprint: string; xpub: string; derivation_path: string } }
			| { kind: "unspendable" };
		recoveries: {
			timelock_blocks: number;
			path: {
				keys: { fingerprint: string; xpub: string; derivation_path: string }[];
				threshold: number;
			};
		}[];
	}) => {
		setSubmitting(true);
		setError(null);
		try {
			const wirePrimary =
				input.primary.kind === "unspendable"
					? { kind: "unspendable" }
					: {
							kind: "keys",
							path: { keys: [input.primary.key], threshold: 1 },
						};
			const walletId = await invoke<string>("cmd_local_create_liana", {
				request: {
					name: basics.name,
					network: basics.network,
					primary: wirePrimary,
					recoveries: input.recoveries,
				},
			});
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase: "" },
			});
			setCreatedId(walletId);
			setStep("done");
		} catch (err) {
			setError(
				typeof err === "string" ? err : "Failed to create Liana wallet",
			);
		} finally {
			setSubmitting(false);
		}
	};

	const submitHardware = async (deviceInfo: DeviceInfo) => {
		setSubmitting(true);
		setError(null);
		try {
			const walletId = await invoke<string>("cmd_local_create_singlesig_hw", {
				request: {
					name: basics.name,
					network: basics.network,
					fingerprint: deviceInfo.fingerprint,
					xpub: deviceInfo.xpub,
					derivation_path: deviceInfo.derivation_path,
				},
			});
			// HW wallets have no on-disk seed; unlock with empty passphrase
			// just loads the BDK store + kicks off auto-sync.
			await invoke("cmd_local_unlock_wallet", {
				request: { wallet_id: walletId, passphrase: "" },
			});
			setCreatedId(walletId);
			setStep("done");
		} catch (err) {
			setError(
				typeof err === "string" ? err : "Failed to create hardware wallet",
			);
		} finally {
			setSubmitting(false);
		}
	};

	const finish = () => {
		if (!createdId) return;
		navigate(`/local/wallets/${createdId}`);
	};

	// Per-method step ordering for the progress indicator. Hardware
	// skips passphrase + mnemonic and goes straight to device collection.
	const stepIndex = (() => {
		if (basics.method === "hardware") {
			if (step === "basics") return 0;
			if (step === "hw") return 1;
			if (step === "done") return 2;
			return 0;
		}
		if (basics.method === "watch_only") {
			if (step === "basics") return 0;
			if (step === "watch_only") return 1;
			if (step === "done") return 2;
			return 0;
		}
		if (basics.method === "multisig") {
			if (step === "basics") return 0;
			if (step === "multisig") return 1;
			if (step === "done") return 2;
			return 0;
		}
		if (basics.method === "liana") {
			if (step === "basics") return 0;
			if (step === "liana") return 1;
			if (step === "done") return 2;
			return 0;
		}
		if (step === "basics") return 0;
		if (step === "passphrase") return 1;
		if (step === "mnemonic") return 2;
		if (step === "done") return 3;
		return 0;
	})();

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
								setStep(
									basics.method === "hardware"
										? "hw"
										: basics.method === "watch_only"
											? "watch_only"
											: basics.method === "multisig"
												? "multisig"
												: basics.method === "liana"
													? "liana"
													: "passphrase",
								);
							}}
						/>
					)}

					{step === "hw" && basics.method === "hardware" && (
						<HardwareStep
							network={basics.network}
							submitting={submitting}
							onBack={() => setStep("basics")}
							onDeviceSelected={submitHardware}
						/>
					)}

					{step === "watch_only" && basics.method === "watch_only" && (
						<WatchOnlyStep
							submitting={submitting}
							onBack={() => setStep("basics")}
							onSubmit={submitWatchOnly}
						/>
					)}

					{step === "multisig" && basics.method === "multisig" && (
						<MultisigStep
							submitting={submitting}
							onBack={() => setStep("basics")}
							onSubmit={submitMultisig}
						/>
					)}

					{step === "liana" && basics.method === "liana" && (
						<LianaStep
							submitting={submitting}
							onBack={() => setStep("basics")}
							onSubmit={submitLiana}
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
			: method === "hardware"
				? ["Basics", "Device", "Done", ""]
				: method === "watch_only"
					? ["Basics", "Descriptors", "Done", ""]
					: method === "multisig"
						? ["Basics", "Cosigners", "Done", ""]
						: method === "liana"
							? ["Basics", "Paths", "Done", ""]
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
					{SUPPORTED_NETWORKS.map((n) => {
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
				<div className="mt-2 grid grid-cols-2 gap-2">
					<MethodCard
						active={value.method === "generate"}
						onClick={() => onChange({ ...value, method: "generate" })}
						title="Generate"
						hint="Mint a fresh BIP39 mnemonic on this device."
					/>
					<MethodCard
						active={value.method === "recover"}
						onClick={() => onChange({ ...value, method: "recover" })}
						title="Recover"
						hint="Restore from an existing 12 / 24-word phrase."
					/>
					<MethodCard
						active={value.method === "hardware"}
						onClick={() => onChange({ ...value, method: "hardware" })}
						title="Hardware"
						hint="Connect a Ledger, Trezor, BitBox, or Coldcard."
					/>
					<MethodCard
						active={value.method === "watch_only"}
						onClick={() => onChange({ ...value, method: "watch_only" })}
						title="Watch-only"
						hint="Import existing descriptors to monitor a wallet."
					/>
					<MethodCard
						active={value.method === "multisig"}
						onClick={() => onChange({ ...value, method: "multisig" })}
						title="Multisig"
						hint="M-of-N watch-only with cosigner xpubs."
					/>
					<MethodCard
						active={value.method === "liana"}
						onClick={() => onChange({ ...value, method: "liana" })}
						title="Liana"
						hint="Primary path + timelocked recovery."
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
					: method === "hardware"
						? "Hardware wallet linked."
						: method === "watch_only"
							? "Watch-only wallet imported."
							: method === "multisig"
								? "Multisig wallet created."
								: method === "liana"
									? "Liana wallet created."
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

function HardwareStep({
	network,
	submitting,
	onBack,
	onDeviceSelected,
}: {
	network: string;
	submitting: boolean;
	onBack: () => void;
	onDeviceSelected: (info: DeviceInfo) => void;
}) {
	return (
		<div className="mt-8 space-y-6">
			<div className="rounded-md border border-border bg-card/40 px-4 py-3">
				<p className="text-[12px] leading-relaxed text-muted-foreground">
					Connect your hardware wallet, unlock it (PIN / passphrase on the
					device), then click{" "}
					<span className="font-medium text-foreground">Discover Devices</span>.
					Once it shows as Supported, continue — the wizard reads its xpub at{" "}
					<span className="font-mono text-[11px] text-foreground">
						{primaryDerivationPath(network)}
					</span>{" "}
					and creates a watch-only wallet here. Your private keys never leave
					the device.
				</p>
			</div>

			<DeviceDiscovery
				network={network}
				derivationPath={primaryDerivationPath(network)}
				onDeviceSelected={onDeviceSelected}
			/>

			<div className="flex items-center gap-3">
				<button
					type="button"
					onClick={onBack}
					disabled={submitting}
					className="flex h-11 items-center rounded-md border border-border bg-background px-5 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
				>
					← Back
				</button>
				{submitting && (
					<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
						Creating wallet…
					</span>
				)}
			</div>
		</div>
	);
}

/** Pull `[fingerprint/path]` origins out of a descriptor string. Used
 * purely to populate the wallet metadata's `fingerprints` field for
 * display — the BDK side parses the descriptor independently. */
function parseFingerprints(descriptor: string): string[] {
	const out = new Set<string>();
	const re = /\[([0-9a-fA-F]{8})\b/g;
	let m: RegExpExecArray | null;
	while ((m = re.exec(descriptor)) !== null) {
		out.add(m[1].toLowerCase());
	}
	return [...out];
}

function WatchOnlyStep({
	submitting,
	onBack,
	onSubmit,
}: {
	submitting: boolean;
	onBack: () => void;
	onSubmit: (d: {
		external: string;
		internal: string;
		fingerprints: string[];
		spendable: boolean;
	}) => void;
}) {
	const [external, setExternal] = useState("");
	const [internal, setInternal] = useState("");
	const [spendable, setSpendable] = useState(false);
	const ready = external.trim().length > 0 && internal.trim().length > 0;

	const submit = (e: React.FormEvent) => {
		e.preventDefault();
		if (!ready || submitting) return;
		const ext = external.trim();
		const int = internal.trim();
		const fingerprints = [
			...new Set([...parseFingerprints(ext), ...parseFingerprints(int)]),
		];
		onSubmit({ external: ext, internal: int, fingerprints, spendable });
	};

	return (
		<form className="mt-8 space-y-6" onSubmit={submit}>
			<div className="rounded-md border border-border bg-card/40 px-4 py-3">
				<p className="text-[12px] leading-relaxed text-muted-foreground">
					Paste the wallet's <span className="font-medium text-foreground">external</span>{" "}
					(receive) and <span className="font-medium text-foreground">internal</span>{" "}
					(change) descriptors. Both should resolve to public-key-only
					expressions — anything containing private keys is rejected at
					import time. Typical Sparrow / Liana / Specter exports give you
					two strings of the form{" "}
					<span className="font-mono text-[11px] text-foreground">
						wpkh([fp/84'/1'/0']xpub.../0/*)
					</span>
					.
				</p>
			</div>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					External descriptor (receive)
				</span>
				<textarea
					value={external}
					onChange={(e) => setExternal(e.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck={false}
					rows={3}
					placeholder="wpkh([fp/84'/1'/0']xpub.../0/*)"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Internal descriptor (change)
				</span>
				<textarea
					value={internal}
					onChange={(e) => setInternal(e.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck={false}
					rows={3}
					placeholder="wpkh([fp/84'/1'/0']xpub.../1/*)"
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
				/>
			</label>

			<label className="flex items-start gap-3 rounded-md border border-border bg-card/40 px-4 py-3 cursor-pointer">
				<input
					type="checkbox"
					checked={spendable}
					onChange={(e) => setSpendable(e.target.checked)}
					className="mt-[2px] h-4 w-4 cursor-pointer accent-primary"
				/>
				<div className="flex-1">
					<div className="text-[13px] font-medium text-foreground">
						Enable spending with hardware wallet
					</div>
					<div className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						Adds a Send button. Signing happens via a connected hardware
						device (Ledger, BitBox, Jade, Coldcard, etc.) or by exporting
						the PSBT to an air-gapped signer. Leave off for a pure read-only
						wallet.
					</div>
				</div>
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
					{submitting ? "Importing…" : "Import wallet"}
				</button>
			</div>
		</form>
	);
}

function MultisigStep({
	submitting,
	onBack,
	onSubmit,
}: {
	submitting: boolean;
	onBack: () => void;
	onSubmit: (input: {
		threshold: number;
		cosigners: { key: string; fingerprint?: string }[];
	}) => void;
}) {
	const [threshold, setThreshold] = useState(2);
	const [n, setN] = useState(3);
	const [keys, setKeys] = useState<string[]>(() => Array(15).fill(""));

	const visibleKeys = keys.slice(0, n);
	const allFilled = visibleKeys.every((k) => k.trim().length > 0);
	const ready = threshold >= 1 && threshold <= n && allFilled;

	const submit = (e: React.FormEvent) => {
		e.preventDefault();
		if (!ready || submitting) return;
		const cosigners = visibleKeys.map((raw) => {
			const key = raw.trim();
			const fingerprint = parseFingerprints(key)[0];
			return { key, fingerprint };
		});
		onSubmit({ threshold, cosigners });
	};

	const setKeyAt = (i: number, v: string) => {
		setKeys((prev) => {
			const next = [...prev];
			next[i] = v;
			return next;
		});
	};

	return (
		<form className="mt-8 space-y-6" onSubmit={submit}>
			<div className="rounded-md border border-border bg-card/40 px-4 py-3">
				<p className="text-[12px] leading-relaxed text-muted-foreground">
					Build an M-of-N multisig wallet from cosigner descriptor keys.
					Paste each cosigner's public key expression — typically{" "}
					<span className="font-mono text-[11px] text-foreground">
						[fp/84'/1'/0']xpub...
					</span>{" "}
					(without the trailing{" "}
					<span className="font-mono text-[11px] text-foreground">/0/*</span>{" "}
					or{" "}
					<span className="font-mono text-[11px] text-foreground">/1/*</span>).
					Resulting addresses use{" "}
					<span className="font-mono text-[11px] text-foreground">
						wsh(sortedmulti(M, k1, …, kN))
					</span>
					— BIP67 ordering, so paste cosigners in any order.
				</p>
			</div>

			<div className="grid grid-cols-2 gap-4">
				<label className="block">
					<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
						Threshold (M)
					</span>
					<input
						type="number"
						min={1}
						max={n}
						value={threshold}
						onChange={(e) =>
							setThreshold(Math.max(1, Math.min(n, Number(e.target.value) || 1)))
						}
						className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
					/>
				</label>
				<label className="block">
					<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
						Cosigners (N)
					</span>
					<input
						type="number"
						min={2}
						max={15}
						value={n}
						onChange={(e) => {
							const next = Math.max(2, Math.min(15, Number(e.target.value) || 2));
							setN(next);
							if (threshold > next) setThreshold(next);
						}}
						className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2.5 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
					/>
				</label>
			</div>

			<div className="space-y-3">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Cosigner keys
				</span>
				{visibleKeys.map((value, i) => (
					<div key={i}>
						<div className="font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground/70">
							Cosigner {String(i + 1).padStart(2, "0")}
						</div>
						<textarea
							value={value}
							onChange={(e) => setKeyAt(i, e.target.value)}
							autoComplete="off"
							autoCapitalize="off"
							spellCheck={false}
							rows={2}
							placeholder="[fp/84'/1'/0']xpub..."
							className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
						/>
					</div>
				))}
			</div>

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
					{submitting ? "Creating…" : `Create ${threshold}-of-${n} multisig`}
				</button>
			</div>
		</form>
	);
}

/** Parse a descriptor key expression of the form `[fp/path]xpub...`
 * (with or without a trailing `/<0;1>/*`, `/0/*`, or `/1/*`) into the
 * components the backend wants. Returns null on malformed input. */
function parseDescriptorKey(
	raw: string,
): { fingerprint: string; xpub: string; derivation_path: string } | null {
	const trimmed = raw.trim();
	const m = trimmed.match(/^\[([0-9a-fA-F]{8})\/(.+?)\](.+)$/);
	if (!m) return null;
	const [, fp, path, rest] = m;
	const xpub = rest
		.replace(/\/<0;1>\/\*$/, "")
		.replace(/\/[01]\/\*$/, "")
		.trim();
	if (!xpub) return null;
	return {
		fingerprint: fp.toLowerCase(),
		xpub,
		derivation_path: `m/${path}`,
	};
}

/** Fixed timelock presets in blocks. ~144 blocks/day on Bitcoin. */
const TIMELOCK_PRESETS: { label: string; blocks: number }[] = [
	{ label: "1 day", blocks: 144 },
	{ label: "1 week", blocks: 1008 },
	{ label: "1 month", blocks: 4320 },
	{ label: "6 months", blocks: 25920 },
	{ label: "1 year", blocks: 52560 },
];

function LianaStep({
	submitting,
	onBack,
	onSubmit,
}: {
	submitting: boolean;
	onBack: () => void;
	onSubmit: (input: {
		primary:
			| { kind: "keys"; key: { fingerprint: string; xpub: string; derivation_path: string } }
			| { kind: "unspendable" };
		recoveries: {
			timelock_blocks: number;
			path: {
				keys: { fingerprint: string; xpub: string; derivation_path: string }[];
				threshold: number;
			};
		}[];
	}) => void;
}) {
	const [primaryMode, setPrimaryMode] = useState<"keys" | "unspendable">("keys");
	const [primaryRaw, setPrimaryRaw] = useState("");
	const [recoveryRaw, setRecoveryRaw] = useState("");
	const [timelockBlocks, setTimelockBlocks] = useState(4320);

	const primaryParsed = useMemo(
		() => parseDescriptorKey(primaryRaw),
		[primaryRaw],
	);
	const recoveryParsed = useMemo(
		() => parseDescriptorKey(recoveryRaw),
		[recoveryRaw],
	);
	const ready =
		recoveryParsed !== null &&
		timelockBlocks > 0 &&
		(primaryMode === "unspendable" || primaryParsed !== null);

	const submit = (e: React.FormEvent) => {
		e.preventDefault();
		if (!ready || submitting) return;
		onSubmit({
			primary:
				primaryMode === "unspendable"
					? { kind: "unspendable" }
					: { kind: "keys", key: primaryParsed! },
			recoveries: [
				{
					timelock_blocks: timelockBlocks,
					path: { keys: [recoveryParsed!], threshold: 1 },
				},
			],
		});
	};

	return (
		<form className="mt-8 space-y-6" onSubmit={submit}>
			<div className="rounded-md border border-border bg-card/40 px-4 py-3">
				<p className="text-[12px] leading-relaxed text-muted-foreground">
					Build a Liana timelocked-recovery wallet. The{" "}
					<span className="font-medium text-foreground">primary</span> key can
					spend immediately. The{" "}
					<span className="font-medium text-foreground">recovery</span> key
					unlocks after the timelock elapses (counted from the last on-chain
					activity for the wallet). Paste each key as a descriptor key
					expression —{" "}
					<span className="font-mono text-[11px] text-foreground">
						[fp/48'/1'/0'/2']xpub...
					</span>{" "}
					— typically collected from a hardware wallet beforehand. v1
					supports one key per path; multi-key paths and hot primaries are
					follow-ups.
				</p>
			</div>

			<div>
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Primary path
				</span>
				<div className="mt-2 grid grid-cols-2 gap-2 rounded-md border border-border bg-card/40 p-1">
					<button
						type="button"
						onClick={() => setPrimaryMode("keys")}
						className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
							primaryMode === "keys"
								? "bg-primary text-primary-foreground"
								: "text-muted-foreground hover:text-foreground"
						}`}
					>
						Use my keys
					</button>
					<button
						type="button"
						onClick={() => setPrimaryMode("unspendable")}
						className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
							primaryMode === "unspendable"
								? "bg-primary text-primary-foreground"
								: "text-muted-foreground hover:text-foreground"
						}`}
					>
						Unspendable
					</button>
				</div>
			</div>

			{primaryMode === "keys" && (
				<label className="block">
					<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
						Primary key
					</span>
					<textarea
						value={primaryRaw}
						onChange={(e) => setPrimaryRaw(e.target.value)}
						autoComplete="off"
						autoCapitalize="off"
						spellCheck={false}
						rows={2}
						placeholder="[fp/48'/1'/0'/2']xpub..."
						className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
					/>
					{primaryRaw.trim().length > 0 && primaryParsed === null && (
						<span className="mt-1 block font-mono text-[10px] uppercase tracking-[0.14em] text-destructive">
							Expected [fingerprint/path]xpub...
						</span>
					)}
				</label>
			)}

			{primaryMode === "unspendable" && (
				<div className="rounded-md border border-amber-500/30 bg-amber-500/[0.05] px-4 py-3 text-[12px] leading-relaxed text-foreground">
					<div className="font-medium">Recovery-only wallet</div>
					<p className="mt-1 text-muted-foreground">
						The primary path will be locked with a provably-unspendable
						NUMS-derived key. Funds can <span className="font-medium text-foreground">only</span> be
						moved after the recovery timelock elapses, using the recovery
						key. Use this for cold-storage / inheritance setups where you
						deliberately want no fast-spending option.
					</p>
				</div>
			)}

			<div>
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Recovery timelock
				</span>
				<div className="mt-2 flex flex-wrap items-center gap-2">
					{TIMELOCK_PRESETS.map((preset) => {
						const active = timelockBlocks === preset.blocks;
						return (
							<button
								key={preset.blocks}
								type="button"
								onClick={() => setTimelockBlocks(preset.blocks)}
								className={`rounded-md border px-3 py-1.5 text-[11px] transition-colors ${
									active
										? "border-primary bg-primary/[0.08] text-foreground"
										: "border-border bg-card text-muted-foreground hover:text-foreground"
								}`}
							>
								{preset.label}
							</button>
						);
					})}
				</div>
				<div className="mt-3 flex items-center gap-2">
					<input
						type="number"
						min={1}
						max={65535}
						value={timelockBlocks}
						onChange={(e) =>
							setTimelockBlocks(
								Math.max(1, Math.min(65535, Number(e.target.value) || 1)),
							)
						}
						className="w-32 rounded-md border border-border bg-background px-3 py-2 text-[13px] text-foreground outline-none transition-colors focus:border-primary"
					/>
					<span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
						blocks ≈ {(timelockBlocks / 144).toFixed(1)} days
					</span>
				</div>
			</div>

			<label className="block">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
					Recovery key
				</span>
				<textarea
					value={recoveryRaw}
					onChange={(e) => setRecoveryRaw(e.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck={false}
					rows={2}
					placeholder="[fp/48'/1'/0'/2']xpub..."
					className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] leading-relaxed text-foreground outline-none transition-colors focus:border-primary"
				/>
				{recoveryRaw.trim().length > 0 && recoveryParsed === null && (
					<span className="mt-1 block font-mono text-[10px] uppercase tracking-[0.14em] text-destructive">
						Expected [fingerprint/path]xpub...
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
					{submitting ? "Creating…" : "Create Liana wallet"}
				</button>
			</div>
		</form>
	);
}
