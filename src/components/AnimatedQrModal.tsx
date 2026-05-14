// Animated QR display modal (QBL-234).
//
// Cycles through a series of QR frames at a fixed cadence so an
// air-gapped signer's camera can read a multi-part PSBT. The format
// (BBQr or UR) is chosen by the caller — the modal itself is
// format-agnostic, it just renders whatever string the caller hands it.

import { QRCodeCanvas } from "qrcode.react";
import { useEffect, useState } from "react";
import { psbtToBbqrFrames, psbtToUrFrames, type QrFormat } from "../lib/psbtQr";

const FRAMES_PER_SECOND = 5;

export default function AnimatedQrModal({
	psbtBase64,
	onClose,
}: {
	psbtBase64: string;
	onClose: () => void;
}) {
	const [format, setFormat] = useState<QrFormat>("bbqr");
	const [frames, setFrames] = useState<string[]>([]);
	const [totalParts, setTotalParts] = useState(0);
	const [frameIdx, setFrameIdx] = useState(0);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		setError(null);
		setFrameIdx(0);
		try {
			const out = format === "bbqr" ? psbtToBbqrFrames(psbtBase64) : psbtToUrFrames(psbtBase64);
			setFrames(out.frames);
			setTotalParts(out.totalParts);
		} catch (err) {
			setError(
				typeof err === "string"
					? err
					: err instanceof Error
						? err.message
						: "Failed to encode PSBT as QR",
			);
			setFrames([]);
		}
	}, [format, psbtBase64]);

	useEffect(() => {
		if (frames.length <= 1) return;
		const interval = setInterval(
			() => setFrameIdx((i) => (i + 1) % frames.length),
			1000 / FRAMES_PER_SECOND,
		);
		return () => clearInterval(interval);
	}, [frames]);

	const currentFrame = frames[frameIdx] ?? "";
	const displayIdx = totalParts > 0 ? (frameIdx % totalParts) + 1 : 1;

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
				className="relative w-full max-w-[480px] rounded-md border border-border bg-card p-6 shadow-2xl"
				onClick={(e) => e.stopPropagation()}
				onMouseDown={(e) => e.stopPropagation()}
			>
				<div className="mb-4 flex items-center justify-between">
					<h3 className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
						§ — Sign on another machine
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

				<div className="mb-4 grid grid-cols-2 gap-2 rounded-md border border-border bg-card/40 p-1">
					<button
						type="button"
						onClick={() => setFormat("bbqr")}
						className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
							format === "bbqr"
								? "bg-primary text-primary-foreground"
								: "text-muted-foreground hover:text-foreground"
						}`}
					>
						BBQr
					</button>
					<button
						type="button"
						onClick={() => setFormat("ur")}
						className={`h-9 rounded-sm font-mono text-[10px] uppercase tracking-[0.18em] transition-colors ${
							format === "ur"
								? "bg-primary text-primary-foreground"
								: "text-muted-foreground hover:text-foreground"
						}`}
					>
						UR
					</button>
				</div>

				<div className="rounded-md border border-border bg-card/40 px-4 py-3 text-[12px] leading-relaxed text-muted-foreground">
					{format === "bbqr" ? (
						<>
							<span className="font-medium text-foreground">BBQr</span> works with Coldcard,
							SeedSigner, Krux. Hold this QR up to the device's camera; it will read the frames as
							they cycle.
						</>
					) : (
						<>
							<span className="font-medium text-foreground">UR</span> works with Sparrow, Foundation
							Passport, Specter, Keystone. Fountain-coded — scan order doesn't matter, the device
							can pick frames out of sequence.
						</>
					)}
				</div>

				{error ? (
					<div className="mt-4 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
						{error}
					</div>
				) : (
					<>
						<div className="mt-4 flex items-center justify-center rounded-md border border-border bg-white p-4">
							{currentFrame && (
								<QRCodeCanvas value={currentFrame} size={320} level="M" includeMargin={false} />
							)}
						</div>

						<div className="mt-3 flex items-center justify-between font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
							<span>
								Frame {frameIdx + 1} / {frames.length}
							</span>
							<span>
								Part {displayIdx} of {totalParts}
							</span>
						</div>
					</>
				)}
			</div>
		</div>
	);
}
