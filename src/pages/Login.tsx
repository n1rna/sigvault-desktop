import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function Login() {
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleLogin = async () => {
		try {
			setIsLoading(true);
			setError(null);
			const result = await invoke("cmd_authenticate");
			console.log("Authentication result:", result);
		} catch (err) {
			console.error("Login error:", err);
			setError(err instanceof Error ? err.message : "Failed to authenticate");
		} finally {
			setIsLoading(false);
		}
	};

	return (
		<div className="flex h-screen w-full items-center justify-center bg-background">
			<div className="w-[90%] max-w-[400px] border border-border bg-card p-12 text-center">
				<h1 className="mb-2 text-2xl font-semibold text-foreground">
					Welcome to SigVault
				</h1>
				<p className="mb-8 text-sm text-muted-foreground">
					Secure Bitcoin Wallet Management
				</p>

				{error && (
					<div className="mb-4 border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
						{error}
					</div>
				)}

				<button
					onClick={handleLogin}
					disabled={isLoading}
					className="mb-4 w-full bg-primary py-3.5 text-base font-semibold text-primary-foreground hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
				>
					{isLoading ? "Connecting..." : "Login with OAuth"}
				</button>

				<p className="mt-4 text-xs text-muted-foreground">
					Click the button above to sign in securely using OAuth
					authentication.
				</p>
			</div>
		</div>
	);
}
