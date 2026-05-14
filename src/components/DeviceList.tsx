import type { DiscoveredDevice } from "../types/hardware";
import { getDeviceFingerprint, isDeviceSupported } from "../types/hardware";
import DeviceCard from "./DeviceCard";

interface DeviceListProps {
	devices: DiscoveredDevice[];
	selectedDevice: DiscoveredDevice | null;
	onSelectDevice: (device: DiscoveredDevice) => void;
	onUnlockDevice?: (device: DiscoveredDevice) => void;
	highlightedFingerprints?: Set<string>;
	signerFingerprints?: Set<string>;
	unlockingDeviceId?: string | null;
	unlockStatus?: Record<string, string>;
}

export default function DeviceList({
	devices,
	selectedDevice,
	onSelectDevice,
	onUnlockDevice,
	highlightedFingerprints,
	signerFingerprints,
	unlockingDeviceId,
	unlockStatus,
}: DeviceListProps) {
	if (devices.length === 0) {
		return (
			<div className="py-12 text-center text-muted-foreground">
				<p>No hardware wallets detected.</p>
				<p className="mt-2 text-sm">Please connect your hardware wallet and try again.</p>
			</div>
		);
	}

	return (
		<div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
			{devices.map((device) => {
				const fingerprint = getDeviceFingerprint(device);
				const isHighlighted = fingerprint ? highlightedFingerprints?.has(fingerprint) : false;
				const isNonSigner =
					signerFingerprints && isDeviceSupported(device) && fingerprint
						? !signerFingerprints.has(fingerprint)
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
						isNonSigner={isNonSigner}
						unlockStatusMessage={unlockStatus?.[device.id]}
					/>
				);
			})}
		</div>
	);
}
