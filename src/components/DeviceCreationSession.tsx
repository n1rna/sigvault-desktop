// Device creation session component

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { DeviceInfo, WalletConfig } from "../types/hardware";
import DeviceDiscovery from "./DeviceDiscovery";

interface DeviceCreationSessionProps {
	network: string;
	derivationPath: string;
	sessionId: string;
	onDeviceSubmitted: () => void;
	walletConfig?: WalletConfig;
}

export default function DeviceCreationSession({
	network,
	derivationPath,
	sessionId,
	onDeviceSubmitted,
	walletConfig,
}: DeviceCreationSessionProps) {
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleDeviceSelected = async (deviceInfo: DeviceInfo) => {
		setSubmitting(true);
		setError(null);

		try {
			const result = await invoke<CommandResult>(
				"cmd_submit_device_registration",
				{
					sessionId: sessionId,
					deviceInfo: {
						xpub: deviceInfo.xpub,
						fingerprint: deviceInfo.fingerprint,
						derivation_path: deviceInfo.derivation_path,
						device_type: deviceInfo.device_type,
					},
				},
			);

			if (result.success) {
				onDeviceSubmitted();
			} else {
				setError(result.message || "Failed to submit device registration");
			}
		} catch (err) {
			console.error("Failed to submit device registration:", err);
			setError(String(err));
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div className="device-creation-session">
			{error && <div className="error-message">{error}</div>}

			{submitting ? (
				<div className="submitting-indicator">
					<div className="spinner" />
					<p>Submitting device registration...</p>
				</div>
			) : (
				<DeviceDiscovery
					network={network}
					derivationPath={derivationPath}
					onDeviceSelected={handleDeviceSelected}
					walletConfig={walletConfig}
				/>
			)}
		</div>
	);
}
