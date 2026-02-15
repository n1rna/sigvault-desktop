// Transaction signing session component

import { useState, useMemo, use } from "react";
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
	const [selectedDevice, setSelectedDevice] = useState<DiscoveredDevice | null>(
		null,
	);
	const [signing, setSigning] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [unlockingDeviceId, setUnlockingDeviceId] = useState<string | null>(
		null,
	);
	const [error, setError] = useState<string | null>(null);

	const walletConfig: WalletConfig = useMemo(() => {
		return {
			descriptor: transactionData.transaction.multipath_descriptor,
		};
	}, [
		transactionData.transaction.multipath_descriptor,
	]);

	// Get the set of fingerprints that can sign this transaction
	const signingFingerprints = useMemo(() => {
		return new Set(
			transactionData.signature_slots.map((slot) => slot.fingerprint),
		);
	}, [transactionData.signature_slots]);

	// Find the matching slot for a device
	const getMatchingSlot = (fingerprint: string): SignatureSlot | undefined => {
		return transactionData.signature_slots.find(
			(slot) => slot.fingerprint === fingerprint,
		);
	};

	// Check if a device can sign (must be supported and have matching fingerprint)
	const canSign = (device: DiscoveredDevice): boolean => {
		if (!isDeviceSupported(device)) return false;
		const fingerprint = getDeviceFingerprint(device);
		return fingerprint ? signingFingerprints.has(fingerprint) : false;
	};

	// Truncate PSBT for display
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
				// Auto-select if only one matching device
				const matchingDevices = result.data.filter(canSign);
				if (matchingDevices.length === 1) {
					setSelectedDevice(matchingDevices[0]);
				}
			} else {
				setError(result.message || "Failed to discover devices");
			}
		} catch (err) {
			console.error("Discovery error:", err);
			setError(String(err));
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
				{
					deviceId: device.id,
					walletConfig,
				},
			);

			if (result.success && result.data) {
				// Update the device in the list with the unlocked state
				setDevices((prevDevices) =>
					prevDevices.map((d) => (d.id === device.id ? result.data! : d)),
				);

				// Auto-select if it's now a valid signer
				if (canSign(result.data)) {
					setSelectedDevice(result.data);
				}
			} else {
				setError(result.message || "Failed to unlock device");
			}
		} catch (err) {
			console.error("Unlock error:", err);
			setError(String(err));
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
			// Sign the PSBT
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

			// Submit the signed PSBT
			const submitResult = await invoke<CommandResult>(
				"cmd_submit_transaction_signature",
				{
					sessionId: sessionId,
					signedPsbt: signResult.data.psbt,
					txid: transactionData.transaction.txid,
					deviceFingerprint: fingerprint,
					deviceDerivationPath: matchingSlot.derivation_path,
				},
			);

			if (submitResult.success) {
				onSignatureSubmitted();
			} else {
				setError(submitResult.message || "Failed to submit signature");
			}
		} catch (err) {
			console.error("Signing error:", err);
			setError(String(err));
		} finally {
			setSigning(false);
			setSubmitting(false);
		}
	};

	// Separate devices into matching (can sign), locked, and other
	const matchingDevices = devices.filter(canSign);
	const otherDevices = devices.filter((d) => !canSign(d));

	const selectedCanSign = selectedDevice && canSign(selectedDevice);

	return (
		<div className="transaction-signing">
			<div className="transaction-details-panel">
				<h2>Transaction Details</h2>

				<div className="transaction-info">
					<div className="info-row">
						<span className="label">Transaction ID:</span>
						<span className="value txid">
							{transactionData.transaction.txid}
						</span>
					</div>

					<div className="psbt-preview">
						<span className="label">PSBT:</span>
						<code className="psbt-data">{truncatedPsbt}</code>
					</div>
				</div>

				<div className="signature-slots">
					<h3>Required Signatures</h3>
					<div className="slots-list">
						{transactionData.signature_slots.map((slot, index) => (
							<div key={`${slot.fingerprint}-${index}`} className="slot-item">
								<span className="fingerprint">{slot.fingerprint}</span>
								<span className="derivation-path">{slot.derivation_path}</span>
							</div>
						))}
					</div>
				</div>
			</div>

			{error && <div className="error-message">{error}</div>}

			<div className="device-discovery-section">
				<div className="discovery-header">
					<h2>Hardware Wallet</h2>
					<button
						type="button"
						onClick={handleDiscover}
						disabled={discovering || signing || submitting}
						className="btn-discover"
					>
						{discovering ? "Discovering..." : "Discover Devices"}
					</button>
				</div>

				{devices.length > 0 && (
					<>
						{matchingDevices.length > 0 && (
							<div className="device-section matching-devices">
								<h3>Authorized Signers</h3>
								<DeviceList
									devices={matchingDevices}
									selectedDevice={selectedDevice}
									onSelectDevice={handleSelectDevice}
									onUnlockDevice={handleUnlockDevice}
									highlightedFingerprints={signingFingerprints}
									unlockingDeviceId={unlockingDeviceId}
								/>
							</div>
						)}

						{otherDevices.length > 0 && (
							<div className="device-section other-devices">
								<h3>Other Devices</h3>
								<p className="help-text">
									These devices are not authorized to sign this transaction, or
									require unlocking first.
								</p>
								<DeviceList
									devices={otherDevices}
									selectedDevice={selectedDevice}
									onSelectDevice={handleSelectDevice}
									onUnlockDevice={handleUnlockDevice}
									unlockingDeviceId={unlockingDeviceId}
								/>
							</div>
						)}

						{matchingDevices.length === 0 && (
							<div className="no-matching-devices">
								<p>
									No authorized devices found. Please connect a device with one
									of the following fingerprints:
								</p>
								<ul>
									{transactionData.signature_slots.map((slot, index) => (
										<li key={`${slot.fingerprint}-${index}`}>
											{slot.fingerprint}
										</li>
									))}
								</ul>
								<p className="help-text">
									If your device is locked, click "Unlock Device" to pair it
									first.
								</p>
							</div>
						)}
					</>
				)}

				{selectedCanSign && (
					<div className="signing-actions">
						<button
							type="button"
							onClick={handleSignTransaction}
							disabled={signing || submitting}
							className="btn-sign"
						>
							{signing
								? "Signing... (Check your device)"
								: submitting
									? "Submitting..."
									: "Sign Transaction"}
						</button>
						<p className="signing-help">
							You will be prompted to confirm the transaction on your hardware
							wallet.
						</p>
					</div>
				)}
			</div>
		</div>
	);
}
