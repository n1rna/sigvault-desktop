import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { SigVaultLogo } from "../components/SigVaultLogo";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import type { CommandResult, EnvironmentConfig, EnvironmentsResponse } from "../types/events";

export default function SelectEnv() {
	const [environments, setEnvironments] = useState<EnvironmentConfig[]>([]);
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

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
		<div className="flex h-full w-full items-center justify-center overflow-y-auto">
			<div className="w-full max-w-[460px] px-10 py-12">
				<div className="flex items-center gap-2.5">
					<SigVaultLogo className="h-7 w-7 text-foreground" />
					<span className="text-[15px] font-semibold tracking-tight text-foreground">SigVault</span>
				</div>

				<h1 className="mt-8 text-[26px] font-semibold tracking-tight text-foreground">
					Choose a network
				</h1>
				<p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
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
						<div className="rounded-md border border-border bg-card px-4 py-3 text-[13px] text-muted-foreground">
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
									className={`flex w-full items-center justify-between rounded-md border px-4 py-3 text-left transition-colors ${
										disabled
											? "cursor-not-allowed border-border/50 bg-muted/20 opacity-60"
											: isSelected
												? "border-primary bg-primary/[0.06]"
												: "border-border bg-card hover:border-primary/50"
									}`}
								>
									<div className="flex flex-col">
										<span className="text-[14px] font-medium text-foreground">{env.name}</span>
										<span className="mt-0.5 text-[12px] text-muted-foreground">
											{env.network} ·{" "}
											<span className="font-mono">
												{env.apiBaseUrl.replace(/^https?:\/\//, "")}
											</span>
										</span>
									</div>
									{disabled ? (
										<Badge variant="secondary">Coming soon</Badge>
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

				<Button
					size="lg"
					className="mt-8 h-12 w-full"
					onClick={handleContinue}
					disabled={!selectedId || submitting || loading}
				>
					{submitting ? (
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
							Applying…
						</>
					) : (
						<>
							Continue
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

				<Button
					variant="ghost"
					size="sm"
					className="mt-3 w-full"
					onClick={loadEnvironments}
					disabled={loading || submitting}
				>
					Refresh list
				</Button>
			</div>
		</div>
	);
}
