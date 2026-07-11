// Local-mode settings (QBL-231).
//
// Renders the per-network electrs URL inputs and default-network picker
// for `cmd_local_get_settings` / `cmd_local_set_settings`. v1 supports
// regtest / signet / testnet4; mainnet is gated off across the stack
// and intentionally absent here.
//
// Live URL validation mirrors the Rust-side `validate_electrs_url`:
// accepts `tcp://host:port`, `ssl://host:port`, or bare `host:port`;
// empty values are tolerated (mean "user hasn't configured this
// network yet"). The backend re-validates at save time and is the
// authoritative source of truth — client-side validation is purely a
// UX nicety.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { SUPPORTED_NETWORKS } from "../../constants/networks";
import type { LocalSettings as LocalSettingsType } from "../../types/events";

/** Mirror of `validate_electrs_url` in `src-tauri/src/local_wallet/settings.rs`.
 * Empty strings are valid here ("not configured yet"). */
function validateUrl(url: string): string | null {
	if (url.length === 0) return null;
	const stripped = url.replace(/^(?:tcp|ssl):\/\//, "");
	const lastColon = stripped.lastIndexOf(":");
	if (lastColon <= 0 || lastColon === stripped.length - 1) {
		return "URL must include host:port — use tcp://host:port, ssl://host:port, or host:port";
	}
	const host = stripped.slice(0, lastColon);
	const portStr = stripped.slice(lastColon + 1);
	if (host.length === 0) return "URL is missing the host";
	const port = Number(portStr);
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		return `Port must be a number between 1 and 65535 (got "${portStr}")`;
	}
	return null;
}

export default function LocalSettings() {
	const navigate = useNavigate();
	const [settings, setSettings] = useState<LocalSettingsType | null>(null);
	const [loading, setLoading] = useState(true);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [savedFlash, setSavedFlash] = useState(false);

	useEffect(() => {
		(async () => {
			try {
				const s = await invoke<LocalSettingsType>("cmd_local_get_settings");
				setSettings(s);
			} catch (err) {
				setError(typeof err === "string" ? err : "Failed to load settings");
			} finally {
				setLoading(false);
			}
		})();
	}, []);

	const updateUrl = (network: string, url: string) => {
		if (!settings) return;
		setSettings({
			...settings,
			electrs_urls: { ...settings.electrs_urls, [network]: url },
		});
	};

	const setDefaultNetwork = (network: string) => {
		if (!settings) return;
		setSettings({ ...settings, default_network: network });
	};

	const fieldErrors = settings
		? Object.fromEntries(
				SUPPORTED_NETWORKS.map((n) => [n.id, validateUrl(settings.electrs_urls[n.id] ?? "")]),
			)
		: {};
	const hasErrors = Object.values(fieldErrors).some((e) => e !== null);

	const save = async () => {
		if (!settings || hasErrors) return;
		setSaving(true);
		setError(null);
		try {
			await invoke("cmd_local_set_settings", { settings });
			setSavedFlash(true);
			setTimeout(() => setSavedFlash(false), 1800);
		} catch (err) {
			setError(typeof err === "string" ? err : "Failed to save settings");
		} finally {
			setSaving(false);
		}
	};

	return (
		<div className="flex h-full w-full flex-col overflow-hidden">
			{/* Header bar */}
			<header className="flex shrink-0 items-center gap-3 border-b border-border bg-card/60 px-6 py-3">
				<button
					type="button"
					onClick={() => navigate("/local/wallets")}
					className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					title="Back to wallets"
				>
					<svg
						className="h-4 w-4"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						<path d="m15 18-6-6 6-6" />
					</svg>
				</button>
				<span className="text-[14px] font-semibold text-foreground">Settings</span>
			</header>

			<div className="flex-1 overflow-y-auto px-6 py-7">
				<div className="mx-auto max-w-2xl space-y-8">
					{loading && (
						<div className="rounded-md border border-border bg-card px-4 py-3 text-[13px] text-muted-foreground">
							Loading settings…
						</div>
					)}

					{error && (
						<div className="flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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

					{settings && (
						<>
							<section>
								<h2 className="text-[13px] font-semibold text-foreground">Default network</h2>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
									Pre-selected on the wallet creation wizard. Mainnet is not available in v1.
								</p>
								<div className="mt-4 grid grid-cols-3 gap-2">
									{SUPPORTED_NETWORKS.map((n) => {
										const selected = settings.default_network === n.id;
										return (
											<button
												key={n.id}
												type="button"
												onClick={() => setDefaultNetwork(n.id)}
												className={`flex flex-col items-start rounded-md border px-3 py-2.5 text-left transition-colors ${
													selected
														? "border-primary bg-primary/[0.06]"
														: "border-border bg-card hover:border-primary/50"
												}`}
											>
												<span className="text-[12px] font-medium text-foreground">{n.label}</span>
												<span className="mt-0.5 text-[11px] text-muted-foreground">{n.hint}</span>
											</button>
										);
									})}
								</div>
							</section>

							<section>
								<h2 className="text-[13px] font-semibold text-foreground">Electrs endpoints</h2>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
									URLs the wallet connects to for sync and broadcast. Accepts
									<span className="mx-1 font-mono text-[11px] text-foreground">
										ssl://host:port
									</span>
									,
									<span className="mx-1 font-mono text-[11px] text-foreground">
										tcp://host:port
									</span>
									, or bare{" "}
									<span className="mx-1 font-mono text-[11px] text-foreground">host:port</span>.
									Leave blank if you haven't set up that network yet.
								</p>
								<div className="mt-4 space-y-4">
									{SUPPORTED_NETWORKS.map((n) => {
										const value = settings.electrs_urls[n.id] ?? "";
										const fieldError = fieldErrors[n.id];
										return (
											<div key={n.id}>
												<div className="flex items-baseline justify-between">
													<label
														htmlFor={`electrs-${n.id}`}
														className="text-[12px] font-medium text-muted-foreground"
													>
														{n.label}
													</label>
													<Badge variant="outline">{n.id}</Badge>
												</div>
												<input
													id={`electrs-${n.id}`}
													type="text"
													value={value}
													onChange={(e) => updateUrl(n.id, e.target.value)}
													placeholder={
														n.id === "regtest"
															? "ssl://ers.regtest.sigvault.org:443"
															: "ssl://your.electrs.host:50002"
													}
													autoComplete="off"
													autoCapitalize="off"
													spellCheck={false}
													className={`mt-2 w-full rounded-md border bg-background px-3 py-2.5 font-mono text-[12px] text-foreground outline-none transition-colors focus:border-primary ${
														fieldError ? "border-destructive/60" : "border-border"
													}`}
												/>
												{fieldError && (
													<div className="mt-1.5 text-[11px] text-destructive">{fieldError}</div>
												)}
											</div>
										);
									})}
								</div>
							</section>

							<div className="flex items-center gap-3">
								<Button size="lg" onClick={save} disabled={saving || hasErrors}>
									{saving ? "Saving…" : "Save settings"}
								</Button>
								{savedFlash && (
									<span className="text-[12px] font-medium text-success">✓ Saved</span>
								)}
								{hasErrors && !saving && (
									<span className="text-[12px] text-muted-foreground">
										Fix the highlighted URLs to enable save
									</span>
								)}
							</div>
						</>
					)}
				</div>
			</div>
		</div>
	);
}
