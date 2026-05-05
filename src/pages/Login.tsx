import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SwitchModeButton from "../components/SwitchModeButton";
import WindowControls from "../components/WindowControls";
import type { EnvironmentConfig, EnvironmentsResponse } from "../types/events";

export function Login() {
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [currentEnv, setCurrentEnv] = useState<EnvironmentConfig | null>(null);
	const [changingEnv, setChangingEnv] = useState(false);

	useEffect(() => {
		(async () => {
			try {
				const resp = await invoke<EnvironmentsResponse>(
					"cmd_list_environments",
				);
				const match = resp?.environments?.find(
					(e) => e.id === resp.selected_id,
				);
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

	const onDrag = useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			try {
				getCurrentWindow().startDragging();
			} catch {
				// no-op outside Tauri (e.g. tests)
			}
		}
	}, []);

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
		<div
			onMouseDown={onDrag}
			className="relative flex h-screen w-full select-none overflow-hidden bg-background"
		>
			<WindowControls />

			{/* ═══ Left brand panel ═══ */}
			<aside className="relative flex w-[44%] flex-col justify-between overflow-hidden border-r border-border bg-primary/[0.04]">
				{/* Background texture */}
				<div className="pointer-events-none absolute inset-0 bg-grid opacity-[0.05]" />
				<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.12]" />
				<div className="pointer-events-none absolute -left-32 top-1/3 h-[420px] w-[420px] rounded-full bg-primary/[0.08] blur-[120px]" />
				<div className="pointer-events-none absolute -right-24 bottom-0 h-[320px] w-[320px] rounded-full bg-accent/[0.08] blur-[100px]" />

				{/* Brand lockup */}
				<div className="relative z-10 flex items-center gap-3 p-10">
					<div className="flex h-9 w-9 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-md">
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" className="h-5 w-5">
							<path d="M12 2 4 6v6c0 5 3.5 9 8 10 4.5-1 8-5 8-10V6z" />
							<path d="m9 12 2 2 4-4" />
						</svg>
					</div>
					<div className="flex flex-col leading-none">
						<span className="font-mono text-[13px] font-bold tracking-[0.22em] text-foreground">
							SIGVAULT
						</span>
						<span className="mt-1.5 font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
							desktop
						</span>
					</div>
				</div>

				{/* Hero copy */}
				<div className="relative z-10 px-10 pb-10">
					<div className="font-mono text-[10px] uppercase tracking-[0.24em] text-accent">
						§ Self-custody
					</div>
					<h2 className="mt-4 text-[32px] font-medium leading-[1.1] tracking-[-0.02em] text-foreground">
						Multisig Bitcoin,
						<br />
						<span className="text-primary">done right.</span>
					</h2>
					<p className="mt-5 max-w-[340px] text-[13px] leading-relaxed text-muted-foreground">
						Coordinate hardware wallets across signers. Your keys never leave
						your devices — SigVault is the conductor, not the custodian.
					</p>

					{/* Feature bullets */}
					<ul className="mt-8 space-y-3">
						{[
							"Ledger, Trezor, BitBox, Coldcard",
							"Remote signing ceremonies",
							"Non-custodial by design",
						].map((label) => (
							<li
								key={label}
								className="flex items-center gap-3 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground"
							>
								<span className="h-px w-5 bg-primary/50" />
								{label}
							</li>
						))}
					</ul>
				</div>

				{/* Footer hairline */}
				<div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent" />
			</aside>

			{/* ═══ Right form column ═══ */}
			<main className="relative flex flex-1 items-center justify-center">
				{/* Subtle dot texture */}
				<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.08]" />

				<div
					className="relative w-full max-w-[400px] px-10"
					onMouseDown={(e) => e.stopPropagation()}
				>
					{/* Section label */}
					<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ 01 — Authentication
					</div>

					<h1 className="mt-4 text-[28px] font-medium leading-[1.15] tracking-[-0.015em] text-foreground">
						Welcome back.
					</h1>
					<p className="mt-3 text-[13px] leading-relaxed text-muted-foreground">
						Sign in with your SigVault account to join sessions and
						coordinate with other signers.
					</p>

					{/* Active environment badge */}
					<div className="mt-6 flex items-center justify-between rounded-md border border-border bg-card px-3.5 py-2.5">
						<div className="flex flex-col">
							<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
								Connecting to
							</span>
							<span className="mt-0.5 text-[12px] font-medium text-foreground">
								{currentEnv ? currentEnv.name : "—"}
								{currentEnv && (
									<span className="ml-1.5 font-mono text-[10px] text-muted-foreground">
										· {currentEnv.network}
									</span>
								)}
							</span>
						</div>
						<button
							type="button"
							onClick={handleChangeEnv}
							disabled={changingEnv || isLoading}
							className="rounded-sm border border-border bg-background px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground transition-colors hover:border-primary/60 hover:text-foreground disabled:opacity-50"
						>
							{changingEnv ? "…" : "Change"}
						</button>
					</div>

					{/* Error */}
					{error && (
						<div className="mt-6 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
							<svg className="mt-0.5 h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
								<circle cx="12" cy="12" r="10" />
								<line x1="12" y1="8" x2="12" y2="12" />
								<line x1="12" y1="16" x2="12.01" y2="16" />
							</svg>
							<span className="leading-snug">{error}</span>
						</div>
					)}

					{/* Primary CTA */}
					<button
						onClick={handleLogin}
						disabled={isLoading}
						className="group mt-8 flex h-12 w-full items-center justify-center gap-2.5 rounded-md bg-primary text-[13px] font-medium tracking-[0.02em] text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0 disabled:hover:shadow-md"
					>
						{isLoading ? (
							<>
								<svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Opening browser…
							</>
						) : (
							<>
								Continue with SigVault
								<svg className="h-4 w-4 transition-transform group-hover:translate-x-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
									<path d="M5 12h14" />
									<path d="m12 5 7 7-7 7" />
								</svg>
							</>
						)}
					</button>

					<p className="mt-4 text-center font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/80">
						opens your browser · single sign-on
					</p>

					{/* Divider */}
					<div className="mt-10 flex items-center gap-4">
						<div className="h-px flex-1 bg-border" />
						<span className="font-mono text-[9px] uppercase tracking-[0.2em] text-muted-foreground/60">
							secure by design
						</span>
						<div className="h-px flex-1 bg-border" />
					</div>

					{/* Footer links */}
					<div className="mt-6 flex items-center justify-center gap-5 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/70">
						<span>no keys stored</span>
						<span className="h-1 w-1 rounded-full bg-muted-foreground/30" />
						<span>end-to-end signed</span>
					</div>

					<div className="mt-6 flex justify-center">
						<SwitchModeButton />
					</div>
				</div>
			</main>
		</div>
	);
}
