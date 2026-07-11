import { useUpdateChecker } from "../hooks/useUpdateChecker";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";

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

	const dotColor = error ? "bg-destructive" : readyToRestart ? "bg-success" : "bg-primary";

	return (
		<div className="pointer-events-none fixed inset-x-0 bottom-5 z-40 flex justify-center px-4">
			<div className="pointer-events-auto flex min-w-[360px] max-w-[720px] items-center gap-3 rounded-full border border-border bg-card/95 py-2 pl-4 pr-2 shadow-lg backdrop-blur-md">
				{/* Status dot */}
				<span className={`h-2 w-2 shrink-0 rounded-full ${dotColor}`} />

				{/* Message */}
				<span className="flex flex-1 items-center gap-2 text-[12px] text-foreground">
					{readyToRestart ? (
						<>
							<Badge variant="success">Installed</Badge>
							<span className="text-muted-foreground">
								Restart to apply{" "}
								<span className="font-medium text-foreground">v{update.version}</span>
							</span>
						</>
					) : downloading ? (
						<>
							<Badge variant="default">Downloading</Badge>
							<span className="font-medium text-foreground">v{update.version}</span>
						</>
					) : error ? (
						<>
							<Badge variant="destructive">Failed</Badge>
							<span className="truncate text-muted-foreground">{error}</span>
						</>
					) : (
						<>
							<Badge variant="default">Update available</Badge>
							<span className="font-medium text-foreground">v{update.version}</span>
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
						<span className="text-[11px] tabular-nums text-muted-foreground">{progress}%</span>
					</div>
				)}

				{/* Action button */}
				{readyToRestart && (
					<Button size="sm" className="rounded-full" onClick={restartApp}>
						<svg
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
							<path d="M3 3v5h5" />
						</svg>
						Restart
					</Button>
				)}

				{!downloading && !readyToRestart && !error && (
					<Button size="sm" className="rounded-full" onClick={startUpdate}>
						Update
						<svg
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<path d="M5 12h14" />
							<path d="m12 5 7 7-7 7" />
						</svg>
					</Button>
				)}

				{error && (
					<Button variant="outline" size="sm" className="rounded-full" onClick={startUpdate}>
						Retry
					</Button>
				)}

				{/* Dismiss */}
				<button
					type="button"
					onClick={dismiss}
					className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					title="Dismiss"
					aria-label="Dismiss update"
				>
					<svg
						className="h-3 w-3"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						<path d="M18 6 6 18" />
						<path d="m6 6 12 12" />
					</svg>
				</button>
			</div>
		</div>
	);
}
