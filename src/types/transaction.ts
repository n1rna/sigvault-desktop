// Transaction signing types

export interface TransactionData {
	psbt: string;
	txid: string;
	wallet_name?: string;
	multipath_descriptor?: string;
}

export interface SignatureSlot {
	fingerprint: string;
	derivation_path: string;
}

export interface TransactionSigningData {
	transaction: TransactionData;
	signature_slots: SignatureSlot[];
}

export interface SignedPsbtResult {
	psbt: string; // Base64 encoded signed PSBT
	fingerprint: string;
	derivation_path: string;
}
