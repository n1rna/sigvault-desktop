import type { HardwareWallet } from "../types/hardware";

interface DeviceCardProps {
	device: HardwareWallet;
	onSelect: (device: HardwareWallet) => void;
	isSelected?: boolean;
}

export default function DeviceCard({
	device,
	onSelect,
	isSelected = false,
}: DeviceCardProps) {

	console.log("Rendering DeviceCard for device:", device);
	return (
		<div
			className={`device-card ${isSelected ? "selected" : ""}`}
			onClick={() => onSelect(device)}
		>
			<div className="device-card-header">
				<h3 className="device-model">{device.model}</h3>
				<span
					className={`connection-status ${device.connected ? "connected" : "disconnected"}`}
				>
					{device.connected ? "Connected" : "Disconnected"}
				</span>
			</div>

			<div className="device-card-body">
				<div className="device-info-row">
					<span className="label">Type:</span>
					<span className="value">{device.device_type}</span>
				</div>
				<div className="device-info-row">
					<span className="label">Fingerprint:</span>
					<span className="value fingerprint">{device.fingerprint}</span>
				</div>
			</div>

			<div className="device-card-footer">
				<button
					type="button"
					className="btn-select"
					disabled={!device.connected}
				>
					{isSelected ? "Selected" : "Select"}
				</button>
			</div>
		</div>
	);
}
