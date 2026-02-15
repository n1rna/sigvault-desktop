import type {
	DiscoveredDevice,
	UnsupportedReason,
} from "../types/hardware";
import {
	isDeviceLocked,
	isDeviceSupported,
	isDeviceUnsupported,
	getDeviceFingerprint,
	getDevicePairingCode,
} from "../types/hardware";

interface DeviceCardProps {
	device: DiscoveredDevice;
	onSelect: (device: DiscoveredDevice) => void;
	onUnlock?: (device: DiscoveredDevice) => void;
	isSelected?: boolean;
	isHighlighted?: boolean;
	isUnlocking?: boolean;
}

function getUnsupportedReasonText(reason: UnsupportedReason): string {
	switch (reason.type) {
		case "Version":
			return `Firmware version too old. Minimum required: ${reason.details.minimal_supported_version}`;
		case "Method":
			return `Device doesn't support required feature: ${reason.details}`;
		case "NotPartOfWallet":
			return `Device fingerprint ${reason.details.fingerprint} is not part of this wallet`;
		case "WrongNetwork":
			return "Device is configured for a different network";
		case "AppNotOpen":
			return "Bitcoin app not open on device";
		case "InitializationError":
			return `Initialization error: ${reason.details}`;
	}
}

export default function DeviceCard({
	device,
	onSelect,
	onUnlock,
	isSelected = false,
	isHighlighted = false,
	isUnlocking = false,
}: DeviceCardProps) {
	const isLocked = isDeviceLocked(device);
	const isSupported = isDeviceSupported(device);
	const isUnsupported = isDeviceUnsupported(device);
	const fingerprint = getDeviceFingerprint(device);
	const pairingCode = getDevicePairingCode(device);

	const classNames = [
		"device-card",
		isSelected ? "selected" : "",
		isHighlighted ? "highlighted" : "",
		isLocked ? "locked" : "",
		isUnsupported ? "unsupported" : "",
	]
		.filter(Boolean)
		.join(" ");

	const handleClick = () => {
		if (isSupported) {
			onSelect(device);
		}
	};

	const handleUnlock = (e: React.MouseEvent) => {
		e.stopPropagation();
		if (isLocked && onUnlock) {
			onUnlock(device);
		}
	};

	return (
		<div className={classNames} onClick={handleClick}>
			<div className="device-card-header">
				<h3 className="device-model">{device.model}</h3>
				<span
					className={`device-state-badge ${
						isSupported ? "supported" : isLocked ? "locked" : "unsupported"
					}`}
				>
					{isSupported ? "Ready" : isLocked ? "Locked" : "Unsupported"}
				</span>
			</div>

			<div className="device-card-body">
				<div className="device-info-row">
					<span className="label">Type:</span>
					<span className="value">{device.device_type}</span>
				</div>

				{fingerprint && (
					<div className="device-info-row">
						<span className="label">Fingerprint:</span>
						<span className="value fingerprint">{fingerprint}</span>
					</div>
				)}

				{device.state.state === "Supported" && device.state.version && (
					<div className="device-info-row">
						<span className="label">Version:</span>
						<span className="value">{device.state.version}</span>
					</div>
				)}

				{device.state.state === "Supported" &&
					device.state.registered !== null &&
					device.state.registered !== undefined && (
						<div className="device-info-row">
							<span className="label">Policy:</span>
							<span className="value">
								{device.state.registered ? "Registered" : "Not registered"}
							</span>
						</div>
					)}

				{isLocked && pairingCode && (
					<div className="pairing-code-container">
						<span className="label">Pairing Code:</span>
						<code className="pairing-code">{pairingCode}</code>
						<p className="pairing-instruction">
							Confirm this code matches your device display
						</p>
					</div>
				)}

				{isUnsupported && device.state.state === "Unsupported" && (
					<div className="unsupported-reason">
						<span className="warning-icon">!</span>
						<span>{getUnsupportedReasonText(device.state.reason)}</span>
					</div>
				)}
			</div>

			<div className="device-card-footer">
				{isSupported && (
					<button
						type="button"
						className="btn-select"
						onClick={handleClick}
					>
						{isSelected ? "Selected" : "Select"}
					</button>
				)}

				{isLocked && onUnlock && (
					<button
						type="button"
						className="btn-unlock"
						onClick={handleUnlock}
						disabled={isUnlocking}
					>
						{isUnlocking ? "Unlocking..." : "Unlock Device"}
					</button>
				)}

				{isUnsupported && (
					<button type="button" className="btn-disabled" disabled>
						Cannot Use
					</button>
				)}
			</div>
		</div>
	);
}
