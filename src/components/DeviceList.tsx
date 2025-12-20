import type { HardwareWallet } from "../types/hardware";
import DeviceCard from "./DeviceCard";

interface DeviceListProps {
	devices: HardwareWallet[];
	selectedDevice: HardwareWallet | null;
	onSelectDevice: (device: HardwareWallet) => void;
}

export default function DeviceList({
	devices,
	selectedDevice,
	onSelectDevice,
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
			{devices.map((device) => (
				<DeviceCard
					key={device.id}
					device={device}
					onSelect={onSelectDevice}
					isSelected={selectedDevice?.id === device.id}
				/>
			))}
		</div>
	);
}
