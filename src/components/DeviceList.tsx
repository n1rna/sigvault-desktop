import type { DiscoveredDevice } from "../types/hardware";
import { getDeviceFingerprint } from "../types/hardware";
import DeviceCard from "./DeviceCard";

interface DeviceListProps {
	devices: DiscoveredDevice[];
	selectedDevice: DiscoveredDevice | null;
	onSelectDevice: (device: DiscoveredDevice) => void;
	onUnlockDevice?: (device: DiscoveredDevice) => void;
	highlightedFingerprints?: Set<string>;
	unlockingDeviceId?: string | null;
}

export default function DeviceList({
	devices,
	selectedDevice,
	onSelectDevice,
	onUnlockDevice,
	highlightedFingerprints,
	unlockingDeviceId,
}: DeviceListProps) {
	if (devices.length === 0) {
		return (
			<div className="device-list-empty">
				<p>No hardware wallets detected.</p>
				<p className="help-text">
					Please connect your hardware wallet and try again.
				</p>
			</div>
		);
	}

	return (
		<div className="device-list">
			{devices.map((device) => {
				const fingerprint = getDeviceFingerprint(device);
				const isHighlighted = fingerprint
					? highlightedFingerprints?.has(fingerprint)
					: false;

				return (
					<DeviceCard
						key={device.id}
						device={device}
						onSelect={onSelectDevice}
						onUnlock={onUnlockDevice}
						isSelected={selectedDevice?.id === device.id}
						isHighlighted={isHighlighted}
						isUnlocking={unlockingDeviceId === device.id}
					/>
				);
			})}
		</div>
	);
}
