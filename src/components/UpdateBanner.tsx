import { useUpdateChecker } from "../hooks/useUpdateChecker";

export default function UpdateBanner() {
	const {
		update,
		downloading,
		progress,
		readyToRestart,
		dismissed,
		error,
		startUpdate,
		restartApp,
		dismiss,
	} = useUpdateChecker();

	if (dismissed || !update) return null;

	const statusColor = error
		? "bg-destructive"
		: readyToRestart
			? "bg-success"
			: "bg-primary";

	return (
		<div className="pointer-events-none fixed inset-x-0 bottom-5 z-40 flex justify-center px-4">
			<div className="pointer-events-auto relative flex min-w-[360px] max-w-[720px] items-center gap-4 overflow-hidden rounded-full border border-border bg-card/95 py-2.5 pl-4 pr-2.5 shadow-lg backdrop-blur-md">
				{/* Top accent hairline */}
				<div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/40 to-transparent" />

				{/* Status dot */}
				<span className="relative flex h-2 w-2 shrink-0">
					<span
						className={`absolute inline-flex h-full w-full animate-ping rounded-full opacity-50 ${statusColor}`}
					/>
					<span
						className={`relative inline-flex h-2 w-2 rounded-full ${statusColor}`}
					/>
				</span>

				{/* Message */}
				<span className="flex flex-1 items-center gap-2 font-mono text-[11px] tracking-wide text-foreground">
					{readyToRestart ? (
						<>
							<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-success">
								installed
							</span>
							<span className="text-muted-foreground">·</span>
							<span>
								restart to apply{" "}
								<span className="font-semibold text-primary">
									v{update.version}
								</span>
							</span>
						</>
					) : downloading ? (
						<>
							<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-primary">
								downloading
							</span>
							<span className="text-muted-foreground">·</span>
							<span className="font-semibold text-primary">
								v{update.version}
							</span>
						</>
					) : error ? (
						<>
							<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-destructive">
								failed
							</span>
							<span className="truncate text-muted-foreground">
								{error}
							</span>
						</>
					) : (
						<>
							<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-primary">
								update available
							</span>
							<span className="text-muted-foreground">·</span>
							<span className="font-semibold text-foreground">
								v{update.version}
							</span>
						</>
					)}
				</span>

				{/* Progress bar */}
				{downloading && (
					<div className="flex items-center gap-2">
						<div className="h-[3px] w-28 overflow-hidden rounded-full bg-primary/15">
							<div
								className="h-full rounded-full bg-primary transition-all"
								style={{ width: `${progress}%` }}
							/>
						</div>
						<span className="font-mono text-[10px] tabular-nums text-muted-foreground">
							{progress}%
						</span>
					</div>
				)}

				{/* Action button */}
				{readyToRestart && (
					<button
						type="button"
						onClick={restartApp}
						className="flex h-7 items-center gap-1.5 rounded-full border border-success/40 bg-success/[0.08] px-3 font-mono text-[9px] uppercase tracking-[0.18em] text-success transition-colors hover:border-success/70 hover:bg-success/[0.14]"
					>
						<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
							<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
							<path d="M3 3v5h5" />
						</svg>
						restart
					</button>
				)}

				{!downloading && !readyToRestart && !error && (
					<button
						type="button"
						onClick={startUpdate}
						className="flex h-7 items-center gap-1.5 rounded-full border border-primary/40 bg-primary/[0.08] px-3 font-mono text-[9px] uppercase tracking-[0.18em] text-foreground transition-colors hover:border-primary/70 hover:bg-primary/[0.14]"
					>
						update
						<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
							<path d="M5 12h14" />
							<path d="m12 5 7 7-7 7" />
						</svg>
					</button>
				)}

				{error && (
					<button
						type="button"
						onClick={startUpdate}
						className="flex h-7 items-center gap-1.5 rounded-full border border-destructive/40 bg-destructive/[0.08] px-3 font-mono text-[9px] uppercase tracking-[0.18em] text-destructive transition-colors hover:border-destructive/70 hover:bg-destructive/[0.14]"
					>
						retry
					</button>
				)}

				{/* Dismiss */}
				<button
					type="button"
					onClick={dismiss}
					className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					title="Dismiss"
					aria-label="Dismiss update"
				>
					<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
						<path d="M18 6 6 18" />
						<path d="m6 6 12 12" />
					</svg>
				</button>
			</div>
		</div>
	);
}
