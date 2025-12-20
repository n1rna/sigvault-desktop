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
