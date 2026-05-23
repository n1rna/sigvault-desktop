// Branded mode chooser (QBL-221).
//
// First-launch screen that picks the top-level app mode. Cloud mode
// continues into the existing env select / OAuth flow; Local mode jumps
// straight to the on-device wallet list. The choice is persisted via
// `cmd_set_app_mode` and can be cleared from any "Switch mode" affordance.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useState } from "react";
import { SigVaultLogo } from "../components/SigVaultLogo";
import WindowControls from "../components/WindowControls";
import type { AppMode } from "../types/events";

export default function ModeChooser() {
	const [busy, setBusy] = useState<AppMode | null>(null);
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

	const choose = async (mode: AppMode) => {
		setError(null);
		setBusy(mode);
		try {
			await invoke("cmd_set_app_mode", { mode });
		} catch (e) {
			setError(typeof e === "string" ? e : "Failed to set mode");
			setBusy(null);
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
			<div className="pointer-events-none absolute left-1/3 top-1/2 h-[420px] w-[420px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/[0.07] blur-[140px]" />
			<div className="pointer-events-none absolute right-1/4 top-2/3 h-[320px] w-[320px] -translate-y-1/2 rounded-full bg-accent/[0.06] blur-[120px]" />

			<main className="relative flex flex-1 items-center justify-center overflow-y-auto px-8 py-12">
				<div className="relative w-full max-w-[760px]" onMouseDown={(e) => e.stopPropagation()}>
					{/* ── Brand mark ── */}
					<div className="flex items-center justify-center gap-3">
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

					{/* ── Heading ── */}
					<div className="mt-12 text-center">
						<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							§ — First time setup
						</div>
						<h1 className="mt-3 text-[30px] font-medium leading-[1.15] tracking-[-0.015em] text-foreground">
							How will you use SigVault?
						</h1>
						<p className="mx-auto mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
							Pick the mode that fits your workflow. You can switch any time from Settings — your
							wallets, sessions, and keys stay where you left them.
						</p>
					</div>

					{error && (
						<div className="mt-8 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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

					{/* ── Mode cards ── */}
					<div className="mt-10 grid grid-cols-1 gap-4 sm:grid-cols-2">
						<ModeCard
							busy={busy === "cloud"}
							disabled={busy != null && busy !== "cloud"}
							accent="primary"
							label="Cloud"
							title="SigVault Cloud"
							subtitle="Coordinate multisig with your team. Sessions, devices, and signing flows mediated by the SigVault server."
							features={[
								"Remote signing ceremonies",
								"Hardware wallet across signers",
								"Cloud-coordinated sessions",
							]}
							icon={
								<svg
									className="h-5 w-5"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<path d="M17.5 19a4.5 4.5 0 0 0 .8-8.93A6 6 0 0 0 6 9a4.5 4.5 0 0 0 0 9" />
								</svg>
							}
							ctaLabel="Continue with Cloud"
							onClick={() => choose("cloud")}
						/>
						<ModeCard
							busy={busy === "local"}
							disabled={busy != null && busy !== "local"}
							accent="accent"
							label="Local"
							title="Standalone Wallet"
							subtitle="Self-custodial Bitcoin wallet running entirely on this device. No account, no cloud, no third party."
							features={[
								"Encrypted on-disk seed",
								"Sign and broadcast on-device",
								"Connects to your own electrs",
							]}
							icon={
								<svg
									className="h-5 w-5"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<rect x="3" y="11" width="18" height="11" rx="2" />
									<path d="M7 11V7a5 5 0 0 1 10 0v4" />
								</svg>
							}
							ctaLabel="Continue Standalone"
							onClick={() => choose("local")}
						/>
					</div>

					<p className="mt-10 text-center font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground/70">
						Your keys never leave your devices · non-custodial by design
					</p>
				</div>
			</main>
		</div>
	);
}

function ModeCard({
	busy,
	disabled,
	accent,
	label,
	title,
	subtitle,
	features,
	icon,
	ctaLabel,
	onClick,
}: {
	busy: boolean;
	disabled: boolean;
	accent: "primary" | "accent";
	label: string;
	title: string;
	subtitle: string;
	features: string[];
	icon: React.ReactNode;
	ctaLabel: string;
	onClick: () => void;
}) {
	const accentBg = accent === "primary" ? "bg-primary/[0.10]" : "bg-accent/[0.10]";
	const accentText = accent === "primary" ? "text-primary" : "text-accent";
	const accentBorder = accent === "primary" ? "hover:border-primary/60" : "hover:border-accent/60";
	return (
		<button
			type="button"
			onClick={onClick}
			disabled={disabled}
			className={`group relative flex flex-col rounded-lg border border-border bg-card p-6 text-left transition-all ${accentBorder} hover:-translate-y-[1px] hover:shadow-lg disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0 disabled:hover:shadow-none`}
		>
			<div className="flex items-start justify-between">
				<div
					className={`flex h-10 w-10 items-center justify-center rounded-md ${accentBg} ${accentText}`}
				>
					{icon}
				</div>
				<span className="font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground/80">
					{label}
				</span>
			</div>

			<h2 className="mt-5 text-[18px] font-medium tracking-[-0.01em] text-foreground">{title}</h2>
			<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">{subtitle}</p>

			<ul className="mt-5 space-y-2">
				{features.map((f) => (
					<li
						key={f}
						className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground"
					>
						<span
							className={`h-px w-4 ${accent === "primary" ? "bg-primary/50" : "bg-accent/50"}`}
						/>
						{f}
					</li>
				))}
			</ul>

			<div className="mt-7 flex items-center justify-between border-t border-border pt-4">
				<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-foreground">
					{busy ? "Setting up…" : ctaLabel}
				</span>
				{busy ? (
					<svg
						className={`h-4 w-4 animate-spin ${accentText}`}
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2.5"
						strokeLinecap="round"
					>
						<path d="M21 12a9 9 0 1 1-6.219-8.56" />
					</svg>
				) : (
					<svg
						className={`h-4 w-4 transition-transform group-hover:translate-x-0.5 ${accentText}`}
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
				)}
			</div>
		</button>
	);
}
