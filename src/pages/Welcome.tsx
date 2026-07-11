// Welcome screen — first-launch / logged-out landing page.
//
// Replaces the old two-button ModeChooser. It nudges the user toward
// signing in to SigVault Cloud (the primary path) while still offering a
// no-account local wallet escape hatch. Both actions call the existing
// `cmd_set_app_mode`, which routes onward:
//   - cloud → SelectEnv / Login (never straight to a session here)
//   - local → LocalWallets
// Window chrome (drag, min/close) is owned by the global TopBar, so this
// page renders only its content below the bar.

import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { SigVaultLogo } from "../components/SigVaultLogo";
import { Badge } from "../components/ui/badge";
import type { AppMode } from "../types/events";

export default function Welcome() {
	const [busy, setBusy] = useState<AppMode | null>(null);
	const [error, setError] = useState<string | null>(null);

	const choose = async (mode: AppMode) => {
		setError(null);
		setBusy(mode);
		try {
			await invoke("cmd_set_app_mode", { mode });
		} catch (e) {
			setError(typeof e === "string" ? e : "Something went wrong");
			setBusy(null);
		}
	};

	return (
		<div className="h-full w-full overflow-y-auto">
			<div className="mx-auto flex min-h-full max-w-[720px] flex-col justify-center px-8 py-12">
				{/* ── Brand mark ── */}
				<div className="flex items-center justify-center gap-2.5">
					<SigVaultLogo className="h-7 w-7 text-foreground" />
					<span className="text-[16px] font-semibold tracking-tight text-foreground">SigVault</span>
				</div>

				{/* ── Heading ── */}
				<div className="mt-10 text-center">
					<h1 className="text-[26px] font-semibold tracking-tight text-foreground text-balance">
						Sign in to get started
					</h1>
					<p className="mx-auto mt-3 max-w-md text-[13px] leading-relaxed text-muted-foreground">
						Sign in to SigVault Cloud to coordinate multisig with your team — or use a standalone
						on-device wallet with no account. You can switch any time from the top bar.
					</p>
				</div>

				{error && (
					<div className="mx-auto mt-8 flex w-full max-w-md items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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

				{/* ── Action cards ── */}
				<div className="mt-10 grid grid-cols-1 gap-4 sm:grid-cols-2">
					<ActionCard
						busy={busy === "cloud"}
						disabled={busy != null && busy !== "cloud"}
						accent="primary"
						label="Recommended"
						title="Sign in to Cloud"
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
						ctaLabel="Continue to sign in"
						onClick={() => choose("cloud")}
					/>
					<ActionCard
						busy={busy === "local"}
						disabled={busy != null && busy !== "local"}
						accent="accent"
						label="No account"
						title="Use a local wallet"
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
						ctaLabel="Continue without an account"
						onClick={() => choose("local")}
					/>
				</div>

				<p className="mt-10 text-center text-[12px] text-muted-foreground/70">
					Your keys never leave your devices · non-custodial by design
				</p>
			</div>
		</div>
	);
}

function ActionCard({
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
	const accentBg = accent === "primary" ? "bg-primary/10" : "bg-accent/10";
	const accentText = accent === "primary" ? "text-primary" : "text-accent";
	const accentBorder = accent === "primary" ? "hover:border-primary/50" : "hover:border-accent/50";
	return (
		<button
			type="button"
			onClick={onClick}
			disabled={disabled}
			className={`group flex flex-col rounded-lg border border-border bg-card p-5 text-left outline-none transition-colors ${accentBorder} focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50`}
		>
			<div className="flex items-start justify-between">
				<div
					className={`flex h-10 w-10 items-center justify-center rounded-md ${accentBg} ${accentText}`}
				>
					{icon}
				</div>
				<Badge variant={accent === "primary" ? "default" : "secondary"}>{label}</Badge>
			</div>

			<h2 className="mt-4 text-[17px] font-semibold tracking-tight text-foreground">{title}</h2>
			<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">{subtitle}</p>

			<ul className="mt-4 space-y-2">
				{features.map((f) => (
					<li key={f} className="flex items-center gap-2.5 text-[12px] text-muted-foreground">
						<svg
							className={`h-3.5 w-3.5 shrink-0 ${accentText}`}
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2.5"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<path d="M20 6 9 17l-5-5" />
						</svg>
						{f}
					</li>
				))}
			</ul>

			<div className="mt-6 flex items-center justify-between border-t border-border pt-4">
				<span className="text-[13px] font-medium text-foreground">
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
