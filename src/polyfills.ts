// Browser shims for Node globals expected by `@ngraveio/bc-ur` (used
// by the QBL-234 animated-PSBT QR flow). Its transitive `util`
// polyfill reads `process.env.NODE_DEBUG` at module load, and bc-ur
// itself calls `Buffer.from / Buffer.alloc` unprefixed.
//
// Must be imported BEFORE any module that transitively pulls bc-ur,
// because ESM evaluates imports depth-first in source order — putting
// these assignments at the bottom of main.tsx is too late.
import { Buffer } from "buffer";

if (typeof window !== "undefined") {
	const w = window as unknown as {
		Buffer?: unknown;
		process?: { env: Record<string, string> };
	};
	if (!w.Buffer) {
		w.Buffer = Buffer;
	}
	if (!w.process) {
		w.process = { env: {} };
	}
}
