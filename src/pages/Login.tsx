import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import WindowControls from "../components/WindowControls";

export function Login() {
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleLogin = async () => {
		try {
			setIsLoading(true);
			setError(null);
			await invoke("cmd_authenticate");
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to authenticate");
		} finally {
			setIsLoading(false);
		}
	};

	return (
		<div className="relative flex h-screen w-full items-center justify-center bg-background">
			<WindowControls />
			<div className="absolute inset-0 bg-dots mask-radial-fade opacity-15" />
			<div className="relative w-[90%] max-w-[400px] rounded-lg border border-border bg-card p-10 text-center">
				<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Authentication
				</div>
				<h1 className="mt-3 text-3xl font-medium tracking-tight text-foreground">
					SigVault
				</h1>
				<p className="mt-2 mb-8 text-sm text-muted-foreground">
					Secure Bitcoin Wallet Management
				</p>

				{error && (
					<div className="mb-6 rounded-md border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
						{error}
					</div>
				)}

				<button
					onClick={handleLogin}
					disabled={isLoading}
					className="mb-4 w-full rounded-md bg-primary py-3.5 text-base font-semibold text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
				>
					{isLoading ? "Connecting…" : "Login with OAuth"}
				</button>

				<p className="mt-4 text-xs text-muted-foreground">
					Click the button above to sign in securely using OAuth
					authentication.
				</p>
			</div>
		</div>
	);
}
