import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HardwareWallet, DeviceInfo } from "../types/hardware";
import type { CommandResult } from "../types/events";
import DeviceList from "./DeviceList";

interface DeviceDiscoveryProps {
	network?: string;
	onDeviceSelected: (deviceInfo: DeviceInfo) => void;
	derivationPath?: string;
}

export default function DeviceDiscovery({
	network = "testnet",
	onDeviceSelected,
	derivationPath,
}: DeviceDiscoveryProps) {
	const [discovering, setDiscovering] = useState(false);
	const [devices, setDevices] = useState<HardwareWallet[]>([]);
	const [selectedDevice, setSelectedDevice] = useState<HardwareWallet | null>(
		null,
	);
	const [extracting, setExtracting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleDiscover = async () => {
		setDiscovering(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<HardwareWallet[]>>(
				"cmd_discover_hardware_wallets",
				{ network },
			);

			if (result.success && result.data) {
				setDevices(result.data);
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

	const handleSelectDevice = (device: HardwareWallet) => {
		setSelectedDevice(device);
	};

	const handleExtractInfo = async () => {
		if (!selectedDevice) return;

		setExtracting(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<DeviceInfo>>(
				"cmd_get_device_xpub",
				{
					fingerprint: selectedDevice.fingerprint,
					derivationPath,
					network,
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
					/>

					{selectedDevice && (
						<div className="device-actions">
							<div className="derivation-path-info">
								{/* <label>Derivation Path:</label>
								<code>{derivationPath}</code> */}
							</div>
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
