// PSBT file round-trip helpers (QBL-234).
//
// The standard `.psbt` on-disk format is binary (BIP-174 magic +
// key-value records). The Rust side carries PSBTs as base64 strings, so
// these helpers do the encode/decode boundary for save/open dialogs.

import { save, open } from "@tauri-apps/plugin-dialog";
import { writeFile, readFile } from "@tauri-apps/plugin-fs";

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

/** Save a base64 PSBT to disk in binary BIP-174 form. Returns the path
 *  written, or `null` if the user cancelled the save dialog. */
export async function savePsbtToFile(
	psbtBase64: string,
	defaultName: string = "transaction.psbt",
): Promise<string | null> {
	const path = await save({
		title: "Save PSBT for signing",
		filters: [{ name: "PSBT", extensions: ["psbt"] }],
		defaultPath: defaultName,
	});
	if (!path) return null;
	await writeFile(path, base64ToBytes(psbtBase64));
	return path;
}

/** Open a `.psbt` file from disk and return it as a base64 string ready
 *  to feed into `cmd_local_sign_psbt_*` / `cmd_local_broadcast_psbt`.
 *  Returns `null` if the user cancelled the open dialog. */
export async function loadPsbtFromFile(): Promise<string | null> {
	const path = await open({
		title: "Open signed PSBT",
		filters: [{ name: "PSBT", extensions: ["psbt"] }],
		multiple: false,
		directory: false,
	});
	if (!path || typeof path !== "string") return null;
	const bytes = await readFile(path);
	return bytesToBase64(bytes);
}
