// Animated QR PSBT helpers (QBL-234).
//
// Two on-the-wire formats are widely used by air-gapped Bitcoin signers:
//
// - **BBQr** — Coldcard / SeedSigner / Krux. Self-describing header,
//   fixed part count, simple to assemble. Format: `B$<encoding><type>
//   <total>/<idx>/<data>`.
// - **UR** (Uniform Resource, BCR-2020-005) — Sparrow / Foundation
//   Passport / Specter / Keystone. Fountain-coded, scan order doesn't
//   matter, supports lossy capture. Format: `ur:crypto-psbt/...`.
//
// We expose a unified `PsbtQrFrames` shape so the UI doesn't need to
// know the format; the user picks one from a toggle and we render
// whatever the chosen encoder produced.
//
// On the decode side, the scanner accepts ANY frame and dispatches by
// prefix sniffing. Both formats are unambiguous in their leading bytes.

import { UR, URDecoder, UREncoder } from "@ngraveio/bc-ur";
import { joinQRs, splitQRs } from "bbqr";
import { Buffer } from "buffer";

export type QrFormat = "bbqr" | "ur";

export type PsbtQrFrames = {
	format: QrFormat;
	frames: string[];
	totalParts: number;
};

function base64ToBytes(b64: string): Uint8Array {
	const bin = atob(b64);
	const out = new Uint8Array(bin.length);
	for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
	return out;
}

function bytesToBase64(bytes: Uint8Array): string {
	let bin = "";
	const chunkSize = 0x8000;
	for (let i = 0; i < bytes.length; i += chunkSize) {
		bin += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
	}
	return btoa(bin);
}

/** Encode a base64 PSBT into BBQr frames. Uses the library's default
 *  Zlib compression; falls back to Base32 internally if compression
 *  doesn't help. The resulting frame count depends on PSBT size. */
export function psbtToBbqrFrames(psbtBase64: string): PsbtQrFrames {
	const raw = base64ToBytes(psbtBase64);
	const result = splitQRs(raw, "P", {});
	return {
		format: "bbqr",
		frames: result.parts,
		totalParts: result.parts.length,
	};
}

/** Encode a base64 PSBT into UR (`crypto-psbt`) frames using a
 *  fountain encoder. We emit roughly `2 * partCount` frames so a
 *  scanner that misses some still has redundancy to reassemble. */
export function psbtToUrFrames(psbtBase64: string): PsbtQrFrames {
	const raw = base64ToBytes(psbtBase64);
	const ur = UR.fromBuffer(Buffer.from(raw));
	const wrapped = new UR(ur.cbor, "crypto-psbt");
	const encoder = new UREncoder(wrapped, 200);
	const baseFrames = encoder.fragmentsLength;
	const totalFrames = Math.max(baseFrames * 2, baseFrames + 2);
	const frames: string[] = [];
	for (let i = 0; i < totalFrames; i++) {
		frames.push(encoder.nextPart());
	}
	return {
		format: "ur",
		frames,
		totalParts: baseFrames,
	};
}

/** Streaming reassembler for incoming QR frames. Sniffs format from
 *  the first frame and routes subsequent frames accordingly. Returns
 *  the decoded PSBT (base64) once enough frames have been received,
 *  or `null` while still collecting. Throws on encoder mismatch
 *  (mixing BBQr and UR frames in one session). */
export class PsbtQrAssembler {
	private format: QrFormat | null = null;
	private bbqrParts = new Map<number, string>();
	private bbqrExpected = 0;
	private urDecoder: URDecoder | null = null;

	get progress(): { received: number; expected: number } {
		if (this.format === "bbqr") {
			return {
				received: this.bbqrParts.size,
				expected: this.bbqrExpected,
			};
		}
		if (this.format === "ur" && this.urDecoder) {
			return {
				received: this.urDecoder.receivedPartIndexes().length,
				expected: this.urDecoder.expectedPartCount(),
			};
		}
		return { received: 0, expected: 0 };
	}

	/** Push one decoded QR string into the assembler. Returns the
	 *  reassembled base64 PSBT when complete, otherwise `null`. */
	push(frame: string): string | null {
		const detected: QrFormat = frame.startsWith("ur:") ? "ur" : "bbqr";
		if (this.format && this.format !== detected) {
			throw new Error(`mixed QR formats in one scan (was ${this.format}, got ${detected})`);
		}
		this.format = detected;
		if (detected === "ur") return this.pushUr(frame);
		return this.pushBbqr(frame);
	}

	private pushBbqr(frame: string): string | null {
		// BBQr header: B$<enc><type><total><idx><data>, all hex-ish
		// minus the data. Total + idx are 2-char base36. We need at
		// least the total upfront to know when we're done; let the
		// library do the parsing.
		if (!frame.startsWith("B$")) return null;
		const totalHex = frame.slice(4, 6);
		const idxHex = frame.slice(6, 8);
		const total = parseInt(totalHex, 36);
		const idx = parseInt(idxHex, 36);
		if (Number.isNaN(total) || Number.isNaN(idx)) return null;
		this.bbqrExpected = total;
		this.bbqrParts.set(idx, frame);
		if (this.bbqrParts.size < total) return null;
		const ordered: string[] = [];
		for (let i = 0; i < total; i++) {
			const part = this.bbqrParts.get(i);
			if (!part) return null;
			ordered.push(part);
		}
		const joined = joinQRs(ordered);
		return bytesToBase64(joined.raw);
	}

	private pushUr(frame: string): string | null {
		if (!this.urDecoder) this.urDecoder = new URDecoder();
		this.urDecoder.receivePart(frame);
		if (!this.urDecoder.isComplete()) return null;
		if (!this.urDecoder.isSuccess()) {
			throw new Error(`UR decode failed: ${this.urDecoder.resultError()}`);
		}
		const ur = this.urDecoder.resultUR();
		const cborBuf = ur.decodeCBOR();
		return bytesToBase64(new Uint8Array(cborBuf));
	}
}
