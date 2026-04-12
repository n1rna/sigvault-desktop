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
			return `${psbt.substring(0, 50)}...${psbt.substring(psbt.length - 50)}`;
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

			// Get any new Ledger HMACs from the in-memory cache to persist
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

	return (
		<div className="flex flex-col gap-6">
			<div className="rounded-lg border border-border bg-card p-6">
				<div className="mb-4">
					<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ Transaction
					</div>
					<h2 className="mt-1 text-lg font-medium tracking-tight text-foreground">
						Transaction Details
					</h2>
				</div>

				<div className="flex flex-col gap-3">
					<div className="flex flex-col gap-1">
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							Transaction ID
						</span>
						<span className="break-all font-mono text-sm tabular-nums text-foreground">
							{transactionData.transaction.txid}
						</span>
					</div>

					<div className="flex flex-col gap-2">
						<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							PSBT
						</span>
						<code className="block break-all rounded-md border border-border bg-background p-3 font-mono text-xs leading-relaxed tabular-nums text-muted-foreground">
							{truncatedPsbt}
						</code>
					</div>
				</div>

				<div className="mt-4 border-t border-border pt-4">
					<div className="mb-3 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ Required Signatures
					</div>
					<div className="grid gap-px overflow-hidden rounded-md bg-border">
						{transactionData.signature_slots.map((slot, index) => (
							<div
								key={`${slot.fingerprint}-${index}`}
								className="flex items-center justify-between gap-4 bg-card px-3 py-2.5"
							>
								<span className="flex items-center gap-2">
									<span className="h-2 w-2 rounded-full bg-primary" />
									<span className="font-mono text-sm tracking-wider text-primary">
										{slot.fingerprint}
									</span>
								</span>
								<span className="font-mono text-xs tabular-nums text-muted-foreground">
									{slot.derivation_path}
								</span>
							</div>
						))}
					</div>
				</div>
			</div>

			{error && (
				<div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
					{error}
				</div>
			)}

			<div className="rounded-lg border border-border bg-card p-6">
				<div className="mb-4 flex items-center justify-between gap-4">
					<div>
						<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							§ Hardware Wallet
						</div>
						<h2 className="mt-1 text-lg font-medium tracking-tight text-foreground">
							Sign with Device
						</h2>
					</div>
					<button
						type="button"
						onClick={handleDiscover}
						disabled={discovering || signing || submitting}
						className="rounded-md bg-primary px-5 py-2.5 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
					>
						{discovering ? (
							<span className="flex items-center gap-2">
								<svg className="h-3.5 w-3.5 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Scanning…
							</span>
						) : "Discover Devices"}
					</button>
				</div>

				{devices.length > 0 && (
					<>
						{!hasMatchingDevice && (
							<div className="mb-4 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
								No authorized signers found. Expected: {transactionData.signature_slots.map((s) => s.fingerprint).join(", ")}
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

				{selectedCanSign && (
					<div className="mt-6 flex flex-col items-center gap-3 border-t border-border pt-6">
						<button
							type="button"
							onClick={handleSignTransaction}
							disabled={signing || submitting}
							className="rounded-md bg-primary px-8 py-3.5 text-base font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
						>
							{signing
								? "Signing… (Check your device)"
								: submitting
									? "Submitting…"
									: "Sign Transaction"}
						</button>
						<p className="text-center text-sm text-muted-foreground">
							You will be prompted to confirm the transaction on your
							hardware wallet.
						</p>
					</div>
				)}
			</div>
		</div>
	);
}
