import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { SigVaultLogo } from "../components/SigVaultLogo";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import type { EnvironmentConfig, EnvironmentsResponse } from "../types/events";

const VALUE_POINTS = [
	"Ledger, Trezor, BitBox, Coldcard",
	"Remote signing ceremonies",
	"Non-custodial by design",
];

export function Login() {
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [currentEnv, setCurrentEnv] = useState<EnvironmentConfig | null>(null);
	const [changingEnv, setChangingEnv] = useState(false);

	useEffect(() => {
		(async () => {
			try {
				const resp = await invoke<EnvironmentsResponse>("cmd_list_environments");
				const match = resp?.environments?.find((e) => e.id === resp.selected_id);
				setCurrentEnv(match ?? null);
			} catch {
				setCurrentEnv(null);
			}
		})();
	}, []);

	const handleChangeEnv = async () => {
		try {
			setChangingEnv(true);
			setError(null);
			// Navigate to the picker without clearing the persisted env —
			// SelectEnv reads `selected_id` off cmd_list_environments and
			// pre-selects the current one. Clearing here would null out
			// `app_state.current_env` and leave the picker defaulting to
			// the first non-coming-soon network in the manifest.
			await invoke("cmd_navigate", { route: "SelectEnv" });
		} catch (err) {
			setError(
				err instanceof Error
					? err.message
					: typeof err === "string"
						? err
						: "Failed to open the environment picker",
			);
		} finally {
			setChangingEnv(false);
		}
	};

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
		<div className="flex h-full w-full overflow-hidden">
			{/* ═══ Left brand panel ═══ */}
			<aside className="hidden w-[44%] flex-col justify-between border-r border-border bg-muted/20 p-10 sm:flex">
				<div className="flex items-center gap-2.5">
					<SigVaultLogo className="h-7 w-7 text-foreground" />
					<span className="text-[15px] font-semibold tracking-tight text-foreground">SigVault</span>
				</div>

				<div>
					<h2 className="text-[26px] font-semibold leading-tight tracking-tight text-foreground text-balance">
						Multisig Bitcoin, done right.
					</h2>
					<p className="mt-4 max-w-[340px] text-[13px] leading-relaxed text-muted-foreground">
						Coordinate hardware wallets across signers. Your keys never leave your devices —
						SigVault is the conductor, not the custodian.
					</p>

					<ul className="mt-7 space-y-2.5">
						{VALUE_POINTS.map((label) => (
							<li
								key={label}
								className="flex items-center gap-2.5 text-[13px] text-muted-foreground"
							>
								<svg
									className="h-3.5 w-3.5 shrink-0 text-primary"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2.5"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<path d="M20 6 9 17l-5-5" />
								</svg>
								{label}
							</li>
						))}
					</ul>
				</div>
			</aside>

			{/* ═══ Right form column ═══ */}
			<main className="flex flex-1 items-center justify-center overflow-y-auto">
				<div className="w-full max-w-[400px] px-10">
					<h1 className="text-[26px] font-semibold tracking-tight text-foreground">Welcome back</h1>
					<p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
						Sign in with your SigVault account to join sessions and coordinate with other signers.
					</p>

					{/* Active environment */}
					<div className="mt-6 flex items-center justify-between rounded-md border border-border bg-card px-3.5 py-2.5">
						<div className="flex flex-col">
							<span className="text-[11px] text-muted-foreground">Connecting to</span>
							<span className="mt-0.5 flex items-center gap-1.5 text-[13px] font-medium text-foreground">
								{currentEnv ? currentEnv.name : "—"}
								{currentEnv && <Badge variant="outline">{currentEnv.network}</Badge>}
							</span>
						</div>
						<Button
							variant="outline"
							size="sm"
							onClick={handleChangeEnv}
							disabled={changingEnv || isLoading}
						>
							{changingEnv ? "…" : "Change"}
						</Button>
					</div>

					{error && (
						<div className="mt-6 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
							<svg
								className="mt-0.5 h-3.5 w-3.5 shrink-0"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<circle cx="12" cy="12" r="10" />
								<line x1="12" y1="8" x2="12" y2="12" />
								<line x1="12" y1="16" x2="12.01" y2="16" />
							</svg>
							<span className="leading-snug">{error}</span>
						</div>
					)}

					<Button size="lg" className="mt-8 h-12 w-full" onClick={handleLogin} disabled={isLoading}>
						{isLoading ? (
							<>
								<svg
									className="animate-spin"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2.5"
									strokeLinecap="round"
								>
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Opening browser…
							</>
						) : (
							<>
								Continue with SigVault
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
							</>
						)}
					</Button>

					<p className="mt-4 text-center text-[12px] text-muted-foreground/80">
						Opens your browser · single sign-on
					</p>
				</div>
			</main>
		</div>
	);
}
