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

	return (
		<div className="fixed right-0 bottom-0 left-0 z-40 border-t border-primary/20 bg-primary/10 px-4 py-2">
			<div className="flex items-center gap-3">
				<span className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-primary" />

				<span className="flex-1 font-mono text-xs text-foreground">
					{readyToRestart ? (
						<>Update installed — restart to apply <span className="text-primary">v{update.version}</span></>
					) : downloading ? (
						<>Downloading <span className="text-primary">v{update.version}</span>…</>
					) : error ? (
						<>Update failed: {error}</>
					) : (
						<><span className="text-primary">v{update.version}</span> is available</>
					)}
				</span>

				{downloading && (
					<div className="flex items-center gap-2">
						<div className="h-1 w-24 overflow-hidden rounded-full bg-primary/20">
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

				{readyToRestart && (
					<button
						type="button"
						onClick={restartApp}
						className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90"
					>
						Restart
					</button>
				)}

				{!downloading && !readyToRestart && !error && (
					<button
						type="button"
						onClick={startUpdate}
						className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90"
					>
						Update
					</button>
				)}

				{error && (
					<button
						type="button"
						onClick={startUpdate}
						className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90"
					>
						Retry
					</button>
				)}

				<button
					type="button"
					onClick={dismiss}
					className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					title="Dismiss"
				>
					<svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
						<path d="M18 6 6 18M6 6l12 12" />
					</svg>
				</button>
			</div>
		</div>
	);
}
