import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import type { CommandResult } from "../types/events";
import type { DeviceInfo, DiscoveredDevice, WalletConfig } from "../types/hardware";
import { getDeviceFingerprint, isDeviceSupported } from "../types/hardware";
import DeviceList from "./DeviceList";
import { Button } from "./ui/button";

interface DeviceDiscoveryProps {
	network?: string;
	/** Fired once the user has discovered + unlocked + extracted info from
	 * a device. The second argument is the `DiscoveredDevice` itself, so
	 * callers that need the runtime `id` (e.g. for `cmd_sign_psbt`) don't
	 * have to re-discover. */
	onDeviceSelected: (deviceInfo: DeviceInfo, device: DiscoveredDevice) => void;
	derivationPath?: string;
	walletConfig?: WalletConfig;
}

export default function DeviceDiscovery({
	onDeviceSelected,
	derivationPath,
	walletConfig,
}: DeviceDiscoveryProps) {
	const { clearActivityLog } = useAppState();
	const [discovering, setDiscovering] = useState(false);
	const [devices, setDevices] = useState<DiscoveredDevice[]>([]);
	const [selectedDevice, setSelectedDevice] = useState<DiscoveredDevice | null>(null);
	const [extracting, setExtracting] = useState(false);
	const [unlockingDeviceId, setUnlockingDeviceId] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	const handleDiscover = async () => {
		setDiscovering(true);
		setError(null);
		clearActivityLog();

		try {
			const result = await invoke<CommandResult<DiscoveredDevice[]>>(
				"cmd_discover_hardware_wallets",
				{ walletConfig },
			);

			if (result.success && result.data) {
				setDevices(result.data);
				if (selectedDevice && !result.data.find((d) => d.id === selectedDevice.id)) {
					setSelectedDevice(null);
				}
			} else {
				setError(result.message || "Failed to discover devices");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setDiscovering(false);
			clearActivityLog();
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
			const result = await invoke<CommandResult<DiscoveredDevice>>("cmd_unlock_device", {
				deviceId: device.id,
				walletConfig,
			});

			if (result.success && result.data) {
				setDevices((prevDevices) =>
					prevDevices.map((d) => (d.id === device.id ? result.data! : d)),
				);

				if (isDeviceSupported(result.data)) {
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

	const handleExtractInfo = async () => {
		if (!selectedDevice || !isDeviceSupported(selectedDevice)) return;

		const fingerprint = getDeviceFingerprint(selectedDevice);
		if (!fingerprint) return;

		setExtracting(true);
		setError(null);

		try {
			const result = await invoke<CommandResult<DeviceInfo>>("cmd_get_device_xpub", {
				fingerprint,
				derivationPath,
			});

			if (result.success && result.data) {
				onDeviceSelected(result.data, selectedDevice);
			} else {
				setError(result.message || "Failed to extract device information");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setExtracting(false);
		}
	};

	const canExtract = selectedDevice && isDeviceSupported(selectedDevice) && !extracting;

	return (
		<div className="flex flex-col gap-6 rounded-lg border border-border bg-card p-6">
			<div className="flex items-center justify-between gap-4">
				<div>
					<h2 className="text-[15px] font-semibold tracking-tight text-foreground">
						Hardware wallet
					</h2>
					<p className="mt-0.5 text-[12px] text-muted-foreground">
						Connect and unlock your device, then discover it.
					</p>
				</div>
				<Button onClick={handleDiscover} disabled={discovering}>
					{discovering ? (
						<>
							<svg
								className="animate-spin"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.5"
							>
								<path d="M21 12a9 9 0 1 1-6.219-8.56" />
							</svg>
							Scanning…
						</>
					) : (
						"Discover devices"
					)}
				</Button>
			</div>

			{error && (
				<div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
					{error}
				</div>
			)}

			{devices.length > 0 && (
				<>
					<DeviceList
						devices={devices}
						selectedDevice={selectedDevice}
						onSelectDevice={handleSelectDevice}
						onUnlockDevice={handleUnlockDevice}
						unlockingDeviceId={unlockingDeviceId}
						unlockStatus={{}}
					/>

					{canExtract && (
						<Button size="lg" className="w-full" onClick={handleExtractInfo} disabled={extracting}>
							{extracting ? "Extracting device info…" : "Continue with selected device"}
						</Button>
					)}
				</>
			)}
		</div>
	);
}
