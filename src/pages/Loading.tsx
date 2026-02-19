export default function Loading() {
	return (
		<div className="flex h-full w-full items-center justify-center p-8">
			<div className="text-center">
				<div className="mx-auto mb-6 h-12 w-12 animate-spin rounded-full border-4 border-muted border-t-primary" />
				<h1 className="text-2xl font-semibold text-foreground">
					Sigvault Desktop
				</h1>
				<p className="text-muted-foreground">Connecting to backend...</p>
			</div>
		</div>
	);
}
