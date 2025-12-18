import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function Login() {
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const handleLogin = async () => {
		try {
			setIsLoading(true);
			setError(null);

			// Start OAuth authentication flow
			// This will open the browser, start a local server for the callback,
			// exchange the code for a token, and authenticate with the backend
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
		<div className="login-container">
			<div className="login-box">
				<h1>Welcome to SigVault</h1>
				<p className="login-subtitle">Secure Bitcoin Wallet Management</p>

				{error && <div className="error-message">{error}</div>}

				<button
					onClick={handleLogin}
					disabled={isLoading}
					className="login-button"
				>
					{isLoading ? "Connecting..." : "Login with OAuth"}
				</button>

				<p className="login-info">
					Click the button above to sign in securely using OAuth authentication.
				</p>
			</div>
		</div>
	);
}
