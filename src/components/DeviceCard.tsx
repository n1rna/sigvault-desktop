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
	unlockStatusMessage?: string;
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
	unlockStatusMessage,
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
		"flex flex-col gap-4 rounded-lg border p-5 transition-colors",
		isSelected
			? "border-primary bg-primary/10"
			: isHighlighted
				? "border-success bg-success/10"
				: isLocked
					? "border-primary/50 bg-primary/5"
					: isUnsupported
						? "border-border bg-muted/50 opacity-70"
						: "border-border bg-card",
		isSupported
			? "cursor-pointer hover:border-primary"
			: isLocked
				? "cursor-default"
				: "cursor-not-allowed",
	].join(" ");

	return (
		<div className={cardClasses} onClick={handleClick}>
			<div className="flex items-center justify-between gap-3">
				<div className="flex items-center gap-2.5">
					<span
						className={`h-2.5 w-2.5 rounded-full ${
							isSupported
								? "bg-success"
								: isLocked
									? "animate-pulse bg-primary"
									: "bg-muted-foreground/40"
						}`}
					/>
					<h3 className="text-base font-medium text-foreground">
						{device.model}
					</h3>
				</div>
				<div className="flex items-center gap-2">
					{isNonSigner && (
						<span className="rounded-md bg-destructive/10 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-destructive">
							Not a signer
						</span>
					)}
					<span
						className={`rounded-md px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider ${
							isSupported
								? "bg-success/15 text-success"
								: isLocked
									? "bg-primary/15 text-primary"
									: "bg-muted text-muted-foreground"
						}`}
					>
						{isSupported ? "Ready" : isLocked ? "Locked" : "Unsupported"}
					</span>
				</div>
			</div>

			<div className="grid gap-px overflow-hidden rounded-md bg-border">
				<div className="flex justify-between gap-4 bg-card px-3 py-2">
					<span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Type</span>
					<span className="font-mono text-xs tabular-nums text-foreground">{device.device_type}</span>
				</div>

				{fingerprint && (
					<div className="flex justify-between gap-4 bg-card px-3 py-2">
						<span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Fingerprint</span>
						<span className="font-mono text-sm tracking-wider text-primary">
							{fingerprint}
						</span>
					</div>
				)}

				{device.state.state === "Supported" && device.state.version && (
					<div className="flex justify-between gap-4 bg-card px-3 py-2">
						<span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Version</span>
						<span className="font-mono text-xs tabular-nums text-foreground">
							{device.state.version}
						</span>
					</div>
				)}

				{device.state.state === "Supported" &&
					device.state.registered !== null &&
					device.state.registered !== undefined && (
						<div className="flex justify-between gap-4 bg-card px-3 py-2">
							<span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Policy</span>
							<span className="flex items-center gap-1.5 font-mono text-xs text-foreground">
								<span className={`h-1.5 w-1.5 rounded-full ${device.state.registered ? "bg-success" : "bg-warning"}`} />
								{device.state.registered ? "Registered" : "Not registered"}
							</span>
						</div>
					)}
			</div>

				{isUnlocking && unlockStatusMessage && (
					<div className="text-sm text-muted-foreground">
						{unlockStatusMessage}
					</div>
				)}

				{isUnlocking && pairingCode && (() => {
					const parts = pairingCode.trim().split(/\s+/);
					const topLine = parts.slice(0, 2).join(" ");
					const bottomLine = parts.slice(2).join(" ");
					return (
						<div className="rounded-md border border-primary/30 bg-background p-4">
							<span className="block font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
								Pairing Code
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
					<div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-3">
						<span className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-destructive/20 text-xs font-bold text-destructive">
							!
						</span>
						<span className="text-sm text-muted-foreground">
							{getUnsupportedReasonText(device.state.reason)}
						</span>
					</div>
				)}

			<div className="flex justify-end border-t border-border pt-3">
				{isSupported && (
					<button
						type="button"
						className="rounded-md bg-primary px-5 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90"
						onClick={handleClick}
					>
						{isSelected ? "Selected" : "Select"}
					</button>
				)}

				{isLocked && onUnlock && (
					<button
						type="button"
						className="rounded-md bg-primary px-5 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
						onClick={handleUnlock}
						disabled={isUnlocking}
					>
						{isUnlocking ? "Unlocking…" : "Unlock Device"}
					</button>
				)}

				{isUnsupported && (
					<button
						type="button"
						className="rounded-md border border-border bg-secondary px-5 py-2 text-sm font-medium text-muted-foreground opacity-50"
						disabled
					>
						Cannot Use
					</button>
				)}
			</div>
		</div>
	);
}
