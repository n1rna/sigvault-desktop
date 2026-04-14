import WindowControls from "../components/WindowControls";

export default function Loading() {
	return (
		<div className="relative flex h-full w-full items-center justify-center overflow-hidden bg-background">
			<WindowControls />

			{/* ── Ambient background ── */}
			<div className="pointer-events-none absolute inset-0 bg-grid mask-radial-fade opacity-[0.12]" />
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.08]" />
			<div className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[420px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/[0.06] blur-[140px]" />

			<div className="relative flex flex-col items-center">
				{/* Brand tile with animated ring */}
				<div className="relative mb-8">
					<div
						className="absolute inset-0 -m-2 animate-spin rounded-full border border-transparent border-t-primary"
						style={{ animationDuration: "1.8s" }}
					/>
					<div className="flex h-14 w-14 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-lg">
						<svg
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2.2"
							strokeLinecap="round"
							strokeLinejoin="round"
							className="h-7 w-7"
						>
							<path d="M12 2 4 6v6c0 5 3.5 9 8 10 4.5-1 8-5 8-10V6z" />
							<path d="m9 12 2 2 4-4" />
						</svg>
					</div>
				</div>

				<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					<span className="h-px w-6 bg-primary/60" />
					§ Initializing
					<span className="h-px w-6 bg-primary/60" />
				</div>

				<h1 className="mt-5 font-mono text-[20px] font-bold tracking-[0.22em] text-foreground">
					SIGVAULT
				</h1>

				<p className="mt-3 text-[12px] text-muted-foreground">
					Connecting to coordinator…
				</p>

				{/* Dot flicker */}
				<div className="mt-8 flex items-center gap-1.5">
					{[0, 1, 2].map((i) => (
						<span
							key={i}
							className="h-1 w-1 animate-pulse rounded-full bg-primary/60"
							style={{ animationDelay: `${i * 0.2}s` }}
						/>
					))}
				</div>
			</div>

			{/* Footer hairline */}
			<div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent" />
		</div>
	);
}
