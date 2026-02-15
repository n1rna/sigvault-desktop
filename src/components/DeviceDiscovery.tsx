import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
	DiscoveredDevice,
	DeviceInfo,
	WalletConfig,
} from "../types/hardware";
import { isDeviceSupported, getDeviceFingerprint } from "../types/hardware";
import type { CommandResult } from "../types/events";
import DeviceList from "./DeviceList";

interface DeviceDiscoveryProps {
	network?: string;
	onDeviceSelected: (deviceInfo: DeviceInfo) => void;
	derivationPath?: string;
	walletConfig?: WalletConfig;
}

export default function DeviceDiscovery({
	onDeviceSelected,
	derivationPath,
	walletConfig,
}: DeviceDiscoveryProps) {
	const [discovering, setDiscovering] = useState(false);
	const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
	const [selectedDevice, setSelectedDevice] =
		useState<DiscoveredDevice | null>(null);
	const [extracting, setExtracting] = useState(false);
	const [unlockingDeviceId, setUnlockingDeviceId] = useState<string | null>(
		null,
	);
	const [error, setError] = useState<string | null>(null);

	const handleDiscover = async () => {
		setDiscovering(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<DiscoveredDevice[]>>(
				"cmd_discover_hardware_wallets",
				{
					walletConfig,
				},
			);

			if (result.success && result.data) {
				setDevices(result.data);
				// Clear selection if the device is no longer available
				if (
					selectedDevice &&
					!result.data.find((d) => d.id === selectedDevice.id)
				) {
					setSelectedDevice(null);
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
					prevDevices.map((d) =>
						d.id === device.id ? result.data! : d,
					),
				);

				// Auto-select the unlocked device if it's now supported
				if (isDeviceSupported(result.data)) {
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

	const handleExtractInfo = async () => {
		if (!selectedDevice || !isDeviceSupported(selectedDevice)) return;

		const fingerprint = getDeviceFingerprint(selectedDevice);
		if (!fingerprint) return;

		setExtracting(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<DeviceInfo>>(
				"cmd_get_device_xpub",
				{
					fingerprint,
					derivationPath,
				},
			);

			if (result.success && result.data) {
				onDeviceSelected(result.data);
			} else {
				setError(result.message || "Failed to extract device information");
			}
		} catch (err) {
			console.error("Extraction error:", err);
			setError(String(err));
		} finally {
			setExtracting(false);
		}
	};

	const canExtract =
		selectedDevice && isDeviceSupported(selectedDevice) && !extracting;

	return (
		<div className="device-discovery">
			<div className="discovery-header">
				<h2>Hardware Wallet Discovery</h2>
				<button
					type="button"
					onClick={handleDiscover}
					disabled={discovering}
					className="btn-discover"
				>
					{discovering ? "Discovering..." : "Discover Devices"}
				</button>
			</div>

			{error && <div className="error-message">{error}</div>}

			{devices.length > 0 && (
				<>
					<DeviceList
						devices={devices}
						selectedDevice={selectedDevice}
						onSelectDevice={handleSelectDevice}
						onUnlockDevice={handleUnlockDevice}
						unlockingDeviceId={unlockingDeviceId}
					/>

					{canExtract && (
						<div className="device-actions">
							<button
								type="button"
								onClick={handleExtractInfo}
								disabled={extracting}
								className="btn-extract"
							>
								{extracting
									? "Extracting Device Info..."
									: "Continue with Selected Device"}
							</button>
						</div>
					)}
				</>
			)}
		</div>
	);
}
