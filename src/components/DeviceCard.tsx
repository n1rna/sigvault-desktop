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
	isNonSigner?: boolean;
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
	isNonSigner = false,
}: DeviceCardProps) {
	const isLocked = isDeviceLocked(device);
	const isSupported = isDeviceSupported(device);
	const isUnsupported = isDeviceUnsupported(device);
	const fingerprint = getDeviceFingerprint(device);
	const pairingCode = getDevicePairingCode(device);

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

	const cardClasses = [
		"flex flex-col gap-4 border-2 p-5",
		isSelected
			? "border-primary bg-primary/10"
			: isHighlighted
				? "border-green-500 bg-green-500/10"
				: isLocked
					? "border-primary bg-primary/10"
					: isUnsupported
						? "border-muted-foreground/50 bg-muted/50 opacity-70"
						: "border-border bg-accent",
		isSupported
			? "cursor-pointer hover:border-primary"
			: isLocked
				? "cursor-default"
				: "cursor-not-allowed",
	].join(" ");

	return (
		<div className={cardClasses} onClick={handleClick}>
			<div className="flex items-center justify-between gap-3">
				<h3 className="text-lg font-semibold text-foreground">
					{device.model}
				</h3>
				<div className="flex items-center gap-2">
					{isNonSigner && (
						<span className="bg-destructive/10 px-2 py-1 text-xs font-medium text-destructive">
							Not a signer
						</span>
					)}
					<span
						className={`px-3 py-1 text-xs font-medium ${
							isSupported
								? "bg-green-500/20 text-green-500"
								: isLocked
									? "bg-primary/20 text-primary"
									: "bg-muted-foreground/20 text-muted-foreground"
						}`}
					>
						{isSupported ? "Ready" : isLocked ? "Locked" : "Unsupported"}
					</span>
				</div>
			</div>

			<div className="flex flex-col gap-2">
				<div className="flex justify-between gap-4">
					<span className="text-sm text-muted-foreground">Type:</span>
					<span className="text-sm font-medium">{device.device_type}</span>
				</div>

				{fingerprint && (
					<div className="flex justify-between gap-4">
						<span className="text-sm text-muted-foreground">
							Fingerprint:
						</span>
						<span className="font-mono text-sm font-medium">
							{fingerprint}
						</span>
					</div>
				)}

				{device.state.state === "Supported" && device.state.version && (
					<div className="flex justify-between gap-4">
						<span className="text-sm text-muted-foreground">Version:</span>
						<span className="text-sm font-medium">
							{device.state.version}
						</span>
					</div>
				)}

				{device.state.state === "Supported" &&
					device.state.registered !== null &&
					device.state.registered !== undefined && (
						<div className="flex justify-between gap-4">
							<span className="text-sm text-muted-foreground">
								Policy:
							</span>
							<span className="text-sm font-medium">
								{device.state.registered
									? "Registered"
									: "Not registered"}
							</span>
						</div>
					)}

				{isUnlocking && pairingCode && (() => {
					const parts = pairingCode.trim().split(/\s+/);
					const topLine = parts.slice(0, 2).join(" ");
					const bottomLine = parts.slice(2).join(" ");
					return (
						<div className="mt-3 border border-primary bg-background p-4">
							<span className="block text-xs text-muted-foreground">
								Pairing Code:
							</span>
							<code className="block py-2 text-center font-mono text-xl font-semibold tracking-widest text-primary">
								{topLine}
								{bottomLine && <br />}
								{bottomLine}
							</code>
							<p className="text-center text-xs text-muted-foreground">
								Confirm this code matches your device display
							</p>
						</div>
					);
				})()}

				{isUnsupported && device.state.state === "Unsupported" && (
					<div className="mt-3 flex items-start gap-2 border border-destructive/30 bg-destructive/10 p-3">
						<span className="flex h-5 w-5 shrink-0 items-center justify-center bg-destructive/20 text-xs font-bold text-destructive">
							!
						</span>
						<span className="text-sm text-muted-foreground">
							{getUnsupportedReasonText(device.state.reason)}
						</span>
					</div>
				)}
			</div>

			<div className="flex justify-end border-t border-border pt-2">
				{isSupported && (
					<button
						type="button"
						className="bg-primary px-5 py-2 text-sm font-medium text-primary-foreground hover:opacity-90"
						onClick={handleClick}
					>
						{isSelected ? "Selected" : "Select"}
					</button>
				)}

				{isLocked && onUnlock && (
					<button
						type="button"
						className="bg-primary px-5 py-2 text-sm font-semibold text-primary-foreground hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
						onClick={handleUnlock}
						disabled={isUnlocking}
					>
						{isUnlocking ? "Unlocking..." : "Unlock Device"}
					</button>
				)}

				{isUnsupported && (
					<button
						type="button"
						className="border border-border bg-secondary px-5 py-2 text-sm font-medium text-muted-foreground opacity-50"
						disabled
					>
						Cannot Use
					</button>
				)}
			</div>
		</div>
	);
}
