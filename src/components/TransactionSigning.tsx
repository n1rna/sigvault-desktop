import { useState, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { DiscoveredDevice, WalletConfig } from "../types/hardware";
import { isDeviceSupported, getDeviceFingerprint } from "../types/hardware";
import type {
	TransactionSigningData,
	SignedPsbtResult,
	SignatureSlot,
} from "../types/transaction";
import DeviceList from "./DeviceList";

interface TransactionSigningProps {
	transactionData: TransactionSigningData;
	sessionId: string;
	onSignatureSubmitted: () => void;
}

function shortTxid(txid: string) {
	if (txid.length <= 18) return txid;
	return `${txid.slice(0, 8)}…${txid.slice(-8)}`;
}

export default function TransactionSigning({
	transactionData,
	sessionId,
	onSignatureSubmitted,
}: TransactionSigningProps) {
	const [discovering, setDiscovering] = useState(false);
	const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
	const [selectedDevice, setSelectedDevice] =
		useState<DiscoveredDevice | null>(null);
	const [signing, setSigning] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [unlockingDeviceId, setUnlockingDeviceId] = useState<string | null>(
		null,
	);
	const [error, setError] = useState<string | null>(null);
	const [psbtExpanded, setPsbtExpanded] = useState(false);

	const walletConfig: WalletConfig = useMemo(() => {
		return {
			name: transactionData.transaction.wallet_name,
			descriptor: transactionData.transaction.multipath_descriptor,
			ledger_hmacs: transactionData.transaction.ledger_hmacs,
		};
	}, [transactionData.transaction.wallet_name, transactionData.transaction.multipath_descriptor, transactionData.transaction.ledger_hmacs]);

	const signingFingerprints = useMemo(() => {
		return new Set(
			transactionData.signature_slots.map((slot) => slot.fingerprint),
		);
	}, [transactionData.signature_slots]);

	const getMatchingSlot = (fingerprint: string): SignatureSlot | undefined => {
		return transactionData.signature_slots.find(
			(slot) => slot.fingerprint === fingerprint,
		);
	};

	const canSign = (device: DiscoveredDevice): boolean => {
		if (!isDeviceSupported(device)) return false;
		const fingerprint = getDeviceFingerprint(device);
		return fingerprint ? signingFingerprints.has(fingerprint) : false;
	};

	const truncatedPsbt = useMemo(() => {
		const psbt = transactionData.transaction.psbt;
		if (psbt.length > 100) {
			return `${psbt.substring(0, 50)}…${psbt.substring(psbt.length - 50)}`;
		}
		return psbt;
	}, [transactionData.transaction.psbt]);

	const handleDiscover = async () => {
		setDiscovering(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<DiscoveredDevice[]>>(
				"cmd_discover_hardware_wallets",
				{ walletConfig },
			);

			if (result.success && result.data) {
				setDevices(result.data);
				const matchingDevices = result.data.filter(canSign);
				if (matchingDevices.length === 1) {
					setSelectedDevice(matchingDevices[0]);
				}
			} else {
				setError(result.message || "Failed to discover devices");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setDiscovering(false);
		}
	};

	const handleSelectDevice = (device: DiscoveredDevice) => {
		if (isDeviceSupported(device)) {
			setSelectedDevice(device);
		}
	};

	const handleUnlockDevice = async (device: DiscoveredDevice) => {
		setUnlockingDeviceId(device.id);
		setError(null);

		try {
			const result = await invoke<CommandResult<DiscoveredDevice>>(
				"cmd_unlock_device",
				{ deviceId: device.id, walletConfig },
			);

			if (result.success && result.data) {
				setDevices((prevDevices) =>
					prevDevices.map((d) =>
						d.id === device.id ? result.data! : d,
					),
				);

				if (canSign(result.data)) {
					setSelectedDevice(result.data);
				}
			} else {
				setError(result.message || "Failed to unlock device");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setUnlockingDeviceId(null);
		}
	};

	const handleSignTransaction = async () => {
		if (!selectedDevice || !isDeviceSupported(selectedDevice)) return;

		const fingerprint = getDeviceFingerprint(selectedDevice);
		if (!fingerprint) return;

		const matchingSlot = getMatchingSlot(fingerprint);
		if (!matchingSlot) {
			setError("Selected device is not authorized to sign this transaction");
			return;
		}

		setSigning(true);
		setError(null);

		try {
			const signResult = await invoke<CommandResult<SignedPsbtResult>>(
				"cmd_sign_psbt",
				{
					deviceId: selectedDevice.id,
					fingerprint,
					psbt: transactionData.transaction.psbt,
					walletConfig,
				},
			);

			if (!signResult.success || !signResult.data) {
				setError(signResult.message || "Failed to sign transaction");
				setSigning(false);
				return;
			}

			setSigning(false);
			setSubmitting(true);

			let ledgerHmacs: Record<string, string> = {};
			try {
				const hmacsResult = await invoke<CommandResult<Record<string, string>>>(
					"cmd_get_ledger_hmacs",
				);
				if (hmacsResult.success && hmacsResult.data) {
					ledgerHmacs = hmacsResult.data;
				}
			} catch {
				// Non-critical: HMACs won't be persisted this time
			}

			const submitResult = await invoke<CommandResult>(
				"cmd_submit_transaction_signature",
				{
					sessionId: sessionId,
					signedPsbt: signResult.data.psbt,
					txid: transactionData.transaction.txid,
					deviceFingerprint: fingerprint,
					deviceDerivationPath: matchingSlot.derivation_path,
					ledgerHmacs: Object.keys(ledgerHmacs).length > 0 ? ledgerHmacs : undefined,
				},
			);

			if (submitResult.success) {
				onSignatureSubmitted();
			} else {
				setError(submitResult.message || "Failed to submit signature");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setSigning(false);
			setSubmitting(false);
		}
	};

	const hasMatchingDevice = devices.some(canSign);
	const selectedCanSign = selectedDevice && canSign(selectedDevice);
	const walletName = transactionData.transaction.wallet_name;

	return (
		<div className="flex flex-col gap-6">
			{/* ══════════════ Transaction card ══════════════ */}
			<section className="relative overflow-hidden rounded-lg border border-border bg-card">
				<div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent" />

				{/* Header */}
				<div className="flex items-start justify-between gap-4 border-b border-border/60 px-6 py-5">
					<div>
						<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							<span className="h-px w-5 bg-primary/60" />
							§ 01 — Transaction
						</div>
						<h2 className="mt-2.5 text-[20px] font-medium tracking-tight text-foreground">
							Review and approve
						</h2>
						{walletName && (
							<p className="mt-1 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
								wallet · {walletName}
							</p>
						)}
					</div>

					<div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/[0.08] text-primary">
						<svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
							<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
							<polyline points="14 2 14 8 20 8" />
							<path d="M9 13h6" />
							<path d="M9 17h3" />
						</svg>
					</div>
				</div>

				{/* Txid */}
				<div className="px-6 py-5">
					<div className="font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
						transaction id
					</div>
					<div className="mt-2 flex items-center gap-3">
						<code className="break-all font-mono text-[13px] tabular-nums text-foreground">
							{shortTxid(transactionData.transaction.txid)}
						</code>
						<span
							className="hidden font-mono text-[10px] text-muted-foreground md:inline"
							title={transactionData.transaction.txid}
						>
							{transactionData.transaction.txid.length} chars
						</span>
					</div>

					{/* PSBT */}
					<div className="mt-5">
						<div className="flex items-center justify-between">
							<div className="font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
								psbt payload
							</div>
							<button
								type="button"
								onClick={() => setPsbtExpanded((v) => !v)}
								className="font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-foreground"
							>
								{psbtExpanded ? "collapse" : "expand"}
							</button>
						</div>
						<code className="mt-2 block max-h-[180px] overflow-y-auto break-all rounded-md border border-border/70 bg-background p-3 font-mono text-[11px] leading-relaxed tabular-nums text-muted-foreground">
							{psbtExpanded
								? transactionData.transaction.psbt
								: truncatedPsbt}
						</code>
					</div>
				</div>

				{/* Signature slots */}
				<div className="border-t border-border/60 px-6 pt-5 pb-6">
					<div className="flex items-center justify-between">
						<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							<span className="h-px w-5 bg-accent/60" />
							§ Required signatures
						</div>
						<span className="font-mono text-[10px] uppercase tracking-[0.16em] tabular-nums text-muted-foreground">
							{transactionData.signature_slots.length}{" "}
							{transactionData.signature_slots.length === 1
								? "signer"
								: "signers"}
						</span>
					</div>

					<div className="mt-3 grid gap-px overflow-hidden rounded-md border border-border bg-border">
						{transactionData.signature_slots.map((slot, index) => (
							<div
								key={`${slot.fingerprint}-${index}`}
								className="flex items-center justify-between gap-4 bg-card px-4 py-3"
							>
								<div className="flex items-center gap-3">
									<span className="relative flex h-2 w-2">
										<span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-50" />
										<span className="relative inline-flex h-2 w-2 rounded-full bg-primary" />
									</span>
									<code className="font-mono text-[13px] font-semibold tracking-[0.08em] tabular-nums text-foreground">
										{slot.fingerprint}
									</code>
								</div>
								<div className="font-mono text-[11px] tabular-nums text-muted-foreground">
									{slot.derivation_path}
								</div>
							</div>
						))}
					</div>
				</div>
			</section>

			{/* ══════════════ Error ══════════════ */}
			{error && (
				<div className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
					<svg className="mt-0.5 h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
						<circle cx="12" cy="12" r="10" />
						<line x1="12" y1="8" x2="12" y2="12" />
						<line x1="12" y1="16" x2="12.01" y2="16" />
					</svg>
					<span className="leading-snug">{error}</span>
				</div>
			)}

			{/* ══════════════ Device card ══════════════ */}
			<section className="relative overflow-hidden rounded-lg border border-border bg-card">
				<div className="flex items-start justify-between gap-4 border-b border-border/60 px-6 py-5">
					<div>
						<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							<span className="h-px w-5 bg-primary/60" />
							§ 02 — Hardware wallet
						</div>
						<h2 className="mt-2.5 text-[20px] font-medium tracking-tight text-foreground">
							Sign with device
						</h2>
						<p className="mt-1 text-[12px] text-muted-foreground">
							Connect your hardware wallet and approve on-device.
						</p>
					</div>

					<button
						type="button"
						onClick={handleDiscover}
						disabled={discovering || signing || submitting}
						className="flex h-10 shrink-0 items-center gap-2 rounded-md border border-primary/30 bg-primary/[0.06] px-4 font-mono text-[10px] uppercase tracking-[0.16em] text-foreground transition-colors hover:border-primary/60 hover:bg-primary/[0.12] disabled:cursor-not-allowed disabled:opacity-60"
					>
						{discovering ? (
							<>
								<svg className="h-3.5 w-3.5 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								scanning
							</>
						) : devices.length === 0 ? (
							<>
								<svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
									<circle cx="11" cy="11" r="8" />
									<path d="m21 21-4.3-4.3" />
								</svg>
								discover
							</>
						) : (
							<>
								<svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
									<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
									<path d="M3 3v5h5" />
									<path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
									<path d="M16 16h5v5" />
								</svg>
								rescan
							</>
						)}
					</button>
				</div>

				<div className="px-6 py-5">
					{devices.length === 0 && !discovering && (
						<div className="flex flex-col items-center justify-center py-10 text-center">
							<div className="flex h-14 w-14 items-center justify-center rounded-full border border-dashed border-border bg-background">
								<svg className="h-6 w-6 text-muted-foreground/50" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
									<rect x="2" y="6" width="20" height="12" rx="2" />
									<circle cx="8" cy="12" r="1.5" />
									<path d="M12 12h6" />
								</svg>
							</div>
							<h3 className="mt-5 text-[13px] font-medium text-foreground">
								No devices scanned yet
							</h3>
							<p className="mt-1.5 max-w-[280px] text-[12px] leading-relaxed text-muted-foreground">
								Plug in your hardware wallet and click{" "}
								<span className="font-mono uppercase tracking-wider">discover</span>.
							</p>
						</div>
					)}

					{devices.length > 0 && (
						<>
							{!hasMatchingDevice && (
								<div className="mb-4 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
									<svg className="mt-0.5 h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
										<circle cx="12" cy="12" r="10" />
										<line x1="12" y1="8" x2="12" y2="12" />
										<line x1="12" y1="16" x2="12.01" y2="16" />
									</svg>
									<span className="leading-snug">
										No authorized signers found. Expected:{" "}
										<span className="font-mono tabular-nums">
											{transactionData.signature_slots
												.map((s) => s.fingerprint)
												.join(", ")}
										</span>
									</span>
								</div>
							)}

							<DeviceList
								devices={devices}
								selectedDevice={selectedDevice}
								onSelectDevice={handleSelectDevice}
								onUnlockDevice={handleUnlockDevice}
								highlightedFingerprints={signingFingerprints}
								signerFingerprints={signingFingerprints}
								unlockingDeviceId={unlockingDeviceId}
							/>
						</>
					)}
				</div>

				{selectedCanSign && (
					<div className="relative border-t border-border/60 bg-primary/[0.03] px-6 py-6">
						<div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent" />
						<div className="flex flex-col items-center gap-4">
							<button
								type="button"
								onClick={handleSignTransaction}
								disabled={signing || submitting}
								className="group flex h-12 items-center gap-2.5 rounded-md bg-primary px-10 text-[13px] font-medium tracking-[0.02em] text-primary-foreground shadow-md transition-all hover:-translate-y-[1px] hover:shadow-lg disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0 disabled:hover:shadow-md"
							>
								{signing ? (
									<>
										<svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
											<path d="M21 12a9 9 0 1 1-6.219-8.56" />
										</svg>
										Awaiting device approval…
									</>
								) : submitting ? (
									<>
										<svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
											<path d="M21 12a9 9 0 1 1-6.219-8.56" />
										</svg>
										Submitting signature…
									</>
								) : (
									<>
										Sign transaction
										<svg className="h-4 w-4 transition-transform group-hover:translate-x-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
											<path d="M5 12h14" />
											<path d="m12 5 7 7-7 7" />
										</svg>
									</>
								)}
							</button>
							<p className="text-center font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
								confirm on device display
							</p>
						</div>
					</div>
				)}
			</section>
		</div>
	);
}
