import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useState } from "react";
import { SigVaultLogo } from "../components/SigVaultLogo";
import SwitchModeButton from "../components/SwitchModeButton";
import WindowControls from "../components/WindowControls";
import type { CommandResult, EnvironmentConfig, EnvironmentsResponse } from "../types/events";

export default function SelectEnv() {
	const [environments, setEnvironments] = useState<EnvironmentConfig[]>([]);
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const onDrag = useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			try {
				getCurrentWindow().startDragging();
			} catch {
				// no-op outside Tauri
			}
		}
	}, []);

	const loadEnvironments = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const resp = await invoke<EnvironmentsResponse>("cmd_list_environments");
			setEnvironments(resp.environments);
			const firstAvailable = resp.environments.find((e) => !e.comingSoon);
			setSelectedId(resp.selected_id ?? firstAvailable?.id ?? null);
		} catch (err) {
			setError(
				err instanceof Error
					? err.message
					: typeof err === "string"
						? err
						: "Failed to load environments",
			);
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		loadEnvironments();
	}, [loadEnvironments]);

	const handleContinue = async () => {
		if (!selectedId) return;
		setSubmitting(true);
		setError(null);
		try {
			const result = await invoke<CommandResult>("cmd_set_environment", {
				envId: selectedId,
			});
			if (!result.success) {
				setError(result.message ?? "Failed to set environment");
			}
		} catch (err) {
			setError(
				err instanceof Error
					? err.message
					: typeof err === "string"
						? err
						: "Failed to set environment",
			);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div
			onMouseDown={onDrag}
			className="relative flex h-screen w-full select-none overflow-hidden bg-background"
		>
			<WindowControls />

			<div className="pointer-events-none absolute inset-0 bg-grid mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.10]" />
			<div className="pointer-events-none absolute left-1/2 top-1/3 h-[420px] w-[420px] -translate-x-1/2 rounded-full bg-primary/[0.06] blur-[140px]" />

			<main className="relative flex flex-1 items-center justify-center">
				<div
					className="relative w-full max-w-[460px] px-10"
					onMouseDown={(e) => e.stopPropagation()}
				>
					<div className="flex items-center gap-3">
						<SigVaultLogo className="h-9 w-9 text-foreground" />
						<div className="flex flex-col leading-none">
							<span className="font-mono text-[13px] font-bold tracking-[0.22em] text-foreground">
								SIGVAULT
							</span>
							<span className="mt-1.5 font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
								desktop
							</span>
						</div>
					</div>

					<div className="mt-10 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ 00 — Environment
					</div>
					<h1 className="mt-4 text-[28px] font-medium leading-[1.15] tracking-[-0.015em] text-foreground">
						Choose a network.
					</h1>
					<p className="mt-3 text-[13px] leading-relaxed text-muted-foreground">
						Pick which SigVault deployment to connect to. You can switch later by signing out.
					</p>

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

					<div className="mt-8 space-y-2">
						{loading && (
							<div className="rounded-md border border-border bg-card px-4 py-3 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
								Loading environments…
							</div>
						)}

						{!loading &&
							environments.map((env) => {
								const disabled = !!env.comingSoon;
								const isSelected = selectedId === env.id;
								return (
									<button
										key={env.id}
										type="button"
										disabled={disabled}
										onClick={() => setSelectedId(env.id)}
										className={`flex w-full items-center justify-between rounded-md border px-4 py-3 text-left transition-all ${
											disabled
												? "cursor-not-allowed border-border/50 bg-muted/20 opacity-60"
												: isSelected
													? "border-primary bg-primary/[0.06] shadow-sm"
													: "border-border bg-card hover:border-primary/60 hover:bg-primary/[0.03]"
										}`}
									>
										<div className="flex flex-col">
											<span className="text-[14px] font-medium text-foreground">{env.name}</span>
											<span className="mt-0.5 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
												{env.network} · {env.apiBaseUrl.replace(/^https?:\/\//, "")}
											</span>
										</div>
										{disabled ? (
											<span className="rounded-sm border border-border bg-muted/40 px-2 py-0.5 font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground">
												Coming soon
											</span>
										) : isSelected ? (
											<svg
												className="h-4 w-4 text-primary"
												viewBox="0 0 24 24"
												fill="none"
												stroke="currentColor"
												strokeWidth="2.5"
												strokeLinecap="round"
												strokeLinejoin="round"
											>
												<path d="M20 6 9 17l-5-5" />
											</svg>
										) : null}
									</button>
								);
							})}
					</div>

					<button
						onClick={handleContinue}
						disabled={!selectedId || submitting || loading}
						className="group mt-8 flex h-12 w-full items-center justify-center gap-2.5 rounded-md bg-primary text-[13px] font-medium tracking-[0.02em] text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0 disabled:hover:shadow-md"
					>
						{submitting ? (
							<>
								<svg
									className="h-4 w-4 animate-spin"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2.5"
									strokeLinecap="round"
								>
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Applying…
							</>
						) : (
							<>
								Continue
								<svg
									className="h-4 w-4 transition-transform group-hover:translate-x-0.5"
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
					</button>

					<button
						type="button"
						onClick={loadEnvironments}
						disabled={loading || submitting}
						className="mt-4 w-full text-center font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/80 hover:text-foreground transition-colors disabled:opacity-50"
					>
						↻ Refresh list
					</button>

					<div className="mt-6 flex justify-center">
						<SwitchModeButton />
					</div>
				</div>
			</main>
		</div>
	);
}
