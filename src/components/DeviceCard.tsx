import type { DiscoveredDevice, UnsupportedReason } from "../types/hardware";
import {
	getDeviceFingerprint,
	getDevicePairingCode,
	isDeviceLocked,
	isDeviceSupported,
	isDeviceUnsupported,
} from "../types/hardware";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";

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

	// Status rail color (left edge accent) — encodes device state at a glance.
	const railColor = isSelected
		? "bg-primary"
		: isHighlighted
			? "bg-success"
			: isLocked
				? "bg-primary/60"
				: isUnsupported
					? "bg-destructive/40"
					: "bg-border";

	const cardClasses = [
		"group relative overflow-hidden rounded-lg border transition-colors",
		isSelected
			? "border-primary/60 bg-primary/[0.06]"
			: isHighlighted
				? "border-success/50 bg-success/[0.05]"
				: isLocked
					? "border-primary/40 bg-primary/[0.03] hover:border-primary/60"
					: isUnsupported
						? "border-border bg-muted/30 opacity-80"
						: "border-border bg-card hover:border-primary/40",
		isSupported ? "cursor-pointer" : isLocked ? "cursor-default" : "cursor-not-allowed",
	].join(" ");

	const statusLabel = isSupported ? "Ready" : isLocked ? "Locked" : "Unsupported";
	const statusVariant = isSupported ? "success" : isLocked ? "default" : "secondary";
	const dotColor = isSupported ? "bg-success" : isLocked ? "bg-primary" : "bg-muted-foreground/40";

	return (
		<div className={cardClasses} onClick={handleClick}>
			{/* Left status rail */}
			<div className={`absolute inset-y-0 left-0 w-[3px] ${railColor}`} />

			<div className="flex flex-col gap-4 py-5 pl-6 pr-5">
				{/* Header: model + status */}
				<div className="flex items-start justify-between gap-3">
					<div className="flex items-center gap-3">
						<div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">
							<svg
								className="h-4 w-4"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.8"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<rect x="4" y="2" width="16" height="20" rx="2" />
								<path d="M8 6h8" />
								<path d="M9 14h6" />
								<circle cx="12" cy="19" r="0.5" />
							</svg>
						</div>
						<div className="flex flex-col">
							<div className="flex items-center gap-2">
								<span className={`h-1.5 w-1.5 rounded-full ${dotColor}`} />
								<h3 className="text-[15px] font-semibold tracking-tight text-foreground">
									{device.model}
								</h3>
							</div>
							<div className="mt-0.5 text-[12px] text-muted-foreground">{device.device_type}</div>
						</div>
					</div>

					<div className="flex items-center gap-2">
						{isNonSigner && <Badge variant="destructive">Not a signer</Badge>}
						<Badge variant={statusVariant}>{statusLabel}</Badge>
					</div>
				</div>

				{/* Metadata grid */}
				<div className="grid gap-px overflow-hidden rounded-md border border-border/70 bg-border">
					{fingerprint && (
						<div className="flex items-center justify-between gap-4 bg-card px-3.5 py-2">
							<span className="text-[12px] text-muted-foreground">Fingerprint</span>
							<code className="font-mono text-[12px] font-semibold tabular-nums text-primary">
								{fingerprint}
							</code>
						</div>
					)}

					{device.state.state === "Supported" && device.state.version && (
						<div className="flex items-center justify-between gap-4 bg-card px-3.5 py-2">
							<span className="text-[12px] text-muted-foreground">Firmware</span>
							<span className="font-mono text-[11px] tabular-nums text-foreground">
								{device.state.version}
							</span>
						</div>
					)}

					{device.state.state === "Supported" &&
						device.state.registered !== null &&
						device.state.registered !== undefined && (
							<div className="flex items-center justify-between gap-4 bg-card px-3.5 py-2">
								<span className="text-[12px] text-muted-foreground">Policy</span>
								<span className="flex items-center gap-1.5 text-[12px] text-foreground">
									<span
										className={`h-1.5 w-1.5 rounded-full ${
											device.state.registered ? "bg-success" : "bg-warning"
										}`}
									/>
									{device.state.registered ? "Registered" : "Not registered"}
								</span>
							</div>
						)}
				</div>

				{/* Status message while unlocking */}
				{isUnlocking && unlockStatusMessage && (
					<div className="flex items-center gap-2 rounded-md border border-primary/20 bg-primary/[0.04] px-3 py-2 text-[12px] text-muted-foreground">
						<svg
							className="h-3 w-3 animate-spin text-primary"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
						>
							<path d="M21 12a9 9 0 1 1-6.219-8.56" />
						</svg>
						{unlockStatusMessage}
					</div>
				)}

				{/* Pairing code — confirm on the device display */}
				{isUnlocking &&
					pairingCode &&
					(() => {
						const parts = pairingCode.trim().split(/\s+/);
						const topLine = parts.slice(0, 2).join(" ");
						const bottomLine = parts.slice(2).join(" ");
						return (
							<div className="rounded-md border border-primary/30 bg-background p-4">
								<div className="text-[12px] font-medium text-muted-foreground">Pairing code</div>
								<code className="mt-2 block text-center font-mono text-[22px] font-semibold tracking-[0.25em] text-primary">
									{topLine}
									{bottomLine && <br />}
									{bottomLine}
								</code>
								<p className="mt-2 text-center text-[11px] text-muted-foreground">
									Confirm on device display
								</p>
							</div>
						);
					})()}

				{/* Unsupported reason */}
				{isUnsupported && device.state.state === "Unsupported" && (
					<div className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3 py-2.5 text-[12px] text-destructive">
						<svg
							className="mt-0.5 h-3.5 w-3.5 shrink-0"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<circle cx="12" cy="12" r="10" />
							<line x1="12" y1="8" x2="12" y2="12" />
							<line x1="12" y1="16" x2="12.01" y2="16" />
						</svg>
						<span className="leading-snug">{getUnsupportedReasonText(device.state.reason)}</span>
					</div>
				)}

				{/* Action button */}
				<div className="flex justify-end border-t border-border/60 pt-3">
					{isSupported && (
						<Button
							type="button"
							variant={isSelected ? "secondary" : "default"}
							size="sm"
							onClick={handleClick}
						>
							{isSelected ? (
								<>
									<svg
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										strokeWidth="2.5"
										strokeLinecap="round"
										strokeLinejoin="round"
									>
										<polyline points="20 6 9 17 4 12" />
									</svg>
									Selected
								</>
							) : (
								<>
									Select
									<svg
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										strokeWidth="2"
										strokeLinecap="round"
										strokeLinejoin="round"
									>
										<path d="M5 12h14" />
										<path d="m12 5 7 7-7 7" />
									</svg>
								</>
							)}
						</Button>
					)}

					{isLocked && onUnlock && (
						<Button type="button" size="sm" onClick={handleUnlock} disabled={isUnlocking}>
							{isUnlocking ? (
								<>
									<svg
										className="animate-spin"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										strokeWidth="2"
										strokeLinecap="round"
									>
										<path d="M21 12a9 9 0 1 1-6.219-8.56" />
									</svg>
									Unlocking
								</>
							) : (
								<>
									<svg
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										strokeWidth="2"
										strokeLinecap="round"
										strokeLinejoin="round"
									>
										<rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
										<path d="M7 11V7a5 5 0 0 1 9.9-1" />
									</svg>
									Unlock
								</>
							)}
						</Button>
					)}

					{isUnsupported && (
						<span className="flex h-8 items-center rounded-md border border-border bg-muted/40 px-3 text-[12px] text-muted-foreground">
							Unavailable
						</span>
					)}
				</div>
			</div>
		</div>
	);
}
