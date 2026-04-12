import WindowControls from "../components/WindowControls";

export default function Loading() {
	return (
		<div className="relative flex h-full w-full items-center justify-center bg-background">
			<WindowControls />
			<div className="absolute inset-0 bg-grid mask-radial-fade opacity-20" />
			<div className="relative text-center">
				<div className="mx-auto mb-8 h-10 w-10 animate-spin rounded-full border-2 border-border border-t-primary" />
				<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Initializing
				</div>
				<h1 className="mt-3 text-3xl font-medium tracking-tight text-foreground">
					SigVault
				</h1>
				<p className="mt-2 text-sm text-muted-foreground">
					Connecting to backend…
				</p>
			</div>
		</div>
	);
}
