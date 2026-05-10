// QR camera scan modal (QBL-234).
//
// Opens the system camera via getUserMedia, decodes incoming QR frames
// with jsQR, and feeds them into a `PsbtQrAssembler` that handles both
// BBQr and UR multi-part payloads. Returns the reassembled base64 PSBT
// once decoding completes. Useful for round-tripping a signed PSBT
// from an air-gapped signer that can only emit QR (no SD card slot, no
// USB on the desktop side, etc.).

import { useEffect, useRef, useState } from "react";
import jsQR from "jsqr";
import { PsbtQrAssembler } from "../lib/psbtQr";

const SCAN_INTERVAL_MS = 200;

export default function QrScanModal({
	onClose,
	onComplete,
}: {
	onClose: () => void;
	onComplete: (psbtBase64: string) => void;
}) {
	const videoRef = useRef<HTMLVideoElement | null>(null);
	const canvasRef = useRef<HTMLCanvasElement | null>(null);
	const assemblerRef = useRef<PsbtQrAssembler>(new PsbtQrAssembler());
	const seenFramesRef = useRef<Set<string>>(new Set());
	const [error, setError] = useState<string | null>(null);
	const [progress, setProgress] = useState({ received: 0, expected: 0 });
	const [streaming, setStreaming] = useState(false);

	useEffect(() => {
		let stopped = false;
		let stream: MediaStream | null = null;
		let scanTimer: ReturnType<typeof setInterval> | null = null;

		const start = async () => {
			try {
				stream = await navigator.mediaDevices.getUserMedia({
					video: { facingMode: "environment" },
					audio: false,
				});
				if (stopped) {
					stream.getTracks().forEach((t) => t.stop());
					return;
				}
				if (videoRef.current) {
					videoRef.current.srcObject = stream;
					await videoRef.current.play();
				}
				setStreaming(true);

				scanTimer = setInterval(() => {
					if (stopped) return;
					const v = videoRef.current;
					const c = canvasRef.current;
					if (!v || !c || v.readyState !== v.HAVE_ENOUGH_DATA) return;
					c.width = v.videoWidth;
					c.height = v.videoHeight;
					const ctx = c.getContext("2d", { willReadFrequently: true });
					if (!ctx) return;
					ctx.drawImage(v, 0, 0, c.width, c.height);
					const img = ctx.getImageData(0, 0, c.width, c.height);
					const code = jsQR(img.data, img.width, img.height);
					if (!code || !code.data) return;
					if (seenFramesRef.current.has(code.data)) return;
					seenFramesRef.current.add(code.data);
					try {
						const result = assemblerRef.current.push(code.data);
						setProgress(assemblerRef.current.progress);
						if (result) {
							stopped = true;
							onComplete(result);
						}
					} catch (err) {
						setError(
							typeof err === "string"
								? err
								: err instanceof Error
									? err.message
									: "QR decode failed",
						);
					}
				}, SCAN_INTERVAL_MS);
			} catch (err) {
				setError(
					err instanceof Error
						? `Camera unavailable: ${err.message}`
						: "Camera unavailable",
				);
			}
		};
		void start();

		return () => {
			stopped = true;
			if (scanTimer) clearInterval(scanTimer);
			if (stream) stream.getTracks().forEach((t) => t.stop());
		};
	}, [onComplete]);

	return (
		<div
			className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
			onClick={onClose}
			onKeyDown={(e) => {
				if (e.key === "Escape") onClose();
			}}
			role="dialog"
			tabIndex={-1}
		>
			<div
				className="relative w-full max-w-[520px] rounded-md border border-border bg-card p-6 shadow-2xl"
				onClick={(e) => e.stopPropagation()}
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="mb-4 flex items-center justify-between">
					<h3 className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
						§ — Scan signed PSBT
					</h3>
					<button
						type="button"
						onClick={onClose}
						className="text-muted-foreground transition-colors hover:text-foreground"
						aria-label="Close"
					>
						<svg
							className="h-4 w-4"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<line x1="18" y1="6" x2="6" y2="18" />
							<line x1="6" y1="6" x2="18" y2="18" />
						</svg>
					</button>
				</div>

				<div className="rounded-md border border-border bg-card/40 px-4 py-3 text-[12px] leading-relaxed text-muted-foreground">
					Point your signing device's QR display at the camera. Both BBQr
					(Coldcard, SeedSigner, Krux) and UR (Sparrow, Passport, Specter,
					Keystone) frames are accepted — the format is detected
					automatically from the first frame.
				</div>

				{error ? (
					<div className="mt-4 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
						{error}
					</div>
				) : (
					<>
						<div className="mt-4 overflow-hidden rounded-md border border-border bg-black">
							<video
								ref={videoRef}
								className="block aspect-square w-full object-cover"
								muted
								playsInline
							/>
						</div>
						<canvas ref={canvasRef} className="hidden" />

						<div className="mt-3 flex items-center justify-between font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							<span>{streaming ? "Scanning…" : "Starting camera…"}</span>
							{progress.expected > 0 && (
								<span>
									{progress.received} / {progress.expected} parts
								</span>
							)}
						</div>
					</>
				)}
			</div>
		</div>
	);
}
