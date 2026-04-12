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
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div className="flex flex-col gap-4">
			{error && (
				<div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
					{error}
				</div>
			)}

			{submitting ? (
				<div className="flex flex-col items-center justify-center rounded-lg border border-border bg-card p-12">
					<div className="mx-auto mb-4 h-12 w-12 animate-spin rounded-full border-4 border-muted border-t-primary" />
					<p className="text-sm text-muted-foreground">
						Submitting device registration…
					</p>
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
