import { SigVaultLogo } from "../components/SigVaultLogo";

export default function Loading() {
	return (
		<div className="flex h-full w-full items-center justify-center">
			<div className="flex flex-col items-center">
				{/* Brand tile with a quiet spinning ring */}
				<div className="relative mb-6">
					<div
						className="absolute inset-0 -m-2 animate-spin rounded-full border border-transparent border-t-primary"
						style={{ animationDuration: "1.8s" }}
					/>
					<div className="flex h-14 w-14 items-center justify-center rounded-lg border border-border bg-card text-foreground">
						<SigVaultLogo className="h-8 w-8" />
					</div>
				</div>

				<h1 className="text-[16px] font-semibold tracking-tight text-foreground">SigVault</h1>
				<p className="mt-1.5 text-[12px] text-muted-foreground">Connecting to coordinator…</p>
			</div>
		</div>
	);
}
