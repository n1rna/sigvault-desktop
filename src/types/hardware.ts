// Device state types matching the Rust backend

// Reasons why a device might be unsupported
export type UnsupportedReason =
	| { type: "Version"; details: { minimal_supported_version: string } }
	| { type: "Method"; details: string }
	| { type: "NotPartOfWallet"; details: { fingerprint: string } }
	| { type: "WrongNetwork" }
	| { type: "AppNotOpen" }
	| { type: "InitializationError"; details: string };

// Device state discriminated union
export type DeviceState =
	| {
			state: "Locked";
			pairing_code?: string | null;
	  }
	| {
			state: "Supported";
			fingerprint: string;
			version?: string | null;
			registered?: boolean | null;
	  }
	| {
			state: "Unsupported";
			reason: UnsupportedReason;
			version?: string | null;
	  };

// A discovered hardware wallet device with its state
export interface DiscoveredDevice {
	id: string;
	device_type: string;
	model: string;
	state: DeviceState;
}

// Helper functions for device state checks
export function isDeviceLocked(device: DiscoveredDevice): boolean {
	return device.state.state === "Locked";
}

export function isDeviceSupported(device: DiscoveredDevice): boolean {
	return device.state.state === "Supported";
}

export function isDeviceUnsupported(device: DiscoveredDevice): boolean {
	return device.state.state === "Unsupported";
}

export function getDeviceFingerprint(
	device: DiscoveredDevice
): string | undefined {
	if (device.state.state === "Supported") {
		return device.state.fingerprint;
	}
	return undefined;
}

export function getDevicePairingCode(
	device: DiscoveredDevice
): string | undefined {
	if (device.state.state === "Locked" && device.state.pairing_code) {
		return device.state.pairing_code;
	}
	return undefined;
}

// Wallet configuration for signing operations
export interface WalletConfig {
	name?: string;
	descriptor?: string;
	hmac?: string;
}

// Legacy types (for backward compatibility)
export interface HardwareWallet {
	id: string;
	device_type: string;
	model: string;
	fingerprint: string;
	connected: boolean;
}

export interface DeviceInfo {
	xpub: string;
	fingerprint: string;
	derivation_path: string;
	device_type: string;
}

export interface DeviceRegistrationData {
	xpub: string;
	fingerprint: string;
	derivation_path: string;
	device_type: string;
}

export interface HwiDiscoveryProgress {
	stage: string;
	message: string;
	devices_found: number;
}

export interface HwiUnlockProgress {
	device_id: string;
	message: string;
}
