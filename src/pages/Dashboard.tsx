import { invoke } from "@tauri-apps/api/core";
import { openUrl as openExternal } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";

interface UserInfo {
	id?: string;
	email?: string;
	name?: string;
	username?: string;
}

const WEBAPP_URL = import.meta.env.VITE_WEBAPP_URL || "https://regtest.sigvault.org";

const resources = [
	{
		label: "Web dashboard",
		hint: "Wallets, devices, policies",
		url: "/dash",
		key: "dash",
	},
	{
		label: "Wallets",
		hint: "Multisig configurations",
		url: "/dash/wallets",
		key: "wallets",
	},
	{
		label: "Devices",
		hint: "Hardware signer registry",
		url: "/dash/devices",
		key: "devices",
	},
	{
		label: "Documentation",
		hint: "Guides and references",
		url: "/docs",
		key: "docs",
	},
	{
		label: "Account settings",
		hint: "Profile, sessions, API",
		url: "/dash/settings",
		key: "settings",
	},
];

function formatGreeting() {
	const h = new Date().getHours();
	if (h < 5) return "Working late";
	if (h < 12) return "Good morning";
	if (h < 18) return "Good afternoon";
	return "Good evening";
}

export default function Dashboard() {
	const [userInfo, setUserInfo] = useState<UserInfo | null>(null);
	const [loading, setLoading] = useState(true);

	useEffect(() => {
		const fetchUserInfo = async () => {
			try {
				const user = await invoke<UserInfo>("cmd_get_current_user");
				setUserInfo(user);
			} catch {
				// User info will remain null
			} finally {
				setLoading(false);
			}
		};

		fetchUserInfo();
	}, []);

	const navigateTo = async (route: string) => {
		try {
			await invoke("cmd_navigate", { route });
		} catch {
			// Navigation handled by backend
		}
	};

	const openUrl = async (url: string) => {
		try {
			await openExternal(url);
		} catch {
			// URL opening handled by OS
		}
	};

	const getDisplayName = () => {
		if (userInfo?.name) return userInfo.name;
		if (userInfo?.username) return userInfo.username;
		if (userInfo?.email) return userInfo.email.split("@")[0];
		return "signer";
	};

	return (
		<div className="relative flex h-full w-full overflow-hidden">
			{/* ── Ambient background ── */}
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute -left-40 top-1/4 h-[420px] w-[420px] rounded-full bg-primary/[0.05] blur-[140px]" />
			<div className="pointer-events-none absolute -right-32 bottom-0 h-[320px] w-[320px] rounded-full bg-accent/[0.05] blur-[120px]" />

			{/* ═══ Left column: hero + primary action ═══ */}
			<section className="relative flex flex-1 flex-col justify-between px-12 py-10">
				{/* Greeting */}
				<div>
					<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						<span className="h-px w-6 bg-primary/60" />§ Overview
					</div>

					<h1 className="mt-5 text-[42px] font-medium leading-[1.05] tracking-[-0.025em] text-foreground">
						{formatGreeting()},
						<br />
						<span className="text-primary">{loading ? "…" : getDisplayName()}</span>
					</h1>

					{!loading && userInfo?.email && (
						<p className="mt-4 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground/70">
							{userInfo.email}
						</p>
					)}
				</div>

				{/* Primary action card */}
				<div className="relative mt-10">
					<div className="mb-3 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ 01 — Primary action
					</div>

					<button
						type="button"
						onClick={() => navigateTo("RemoteSessions")}
						className="group relative flex w-full items-center gap-5 overflow-hidden rounded-lg border border-primary/25 bg-primary/[0.05] p-5 text-left transition-all hover:-translate-y-[1px] hover:border-primary/50 hover:bg-primary/[0.09] hover:shadow-lg"
					>
						<div className="pointer-events-none absolute inset-0 bg-grid-sm opacity-[0.05]" />
						<div className="pointer-events-none absolute -right-10 top-1/2 h-[140px] w-[140px] -translate-y-1/2 rounded-full bg-primary/[0.12] blur-[60px] transition-opacity group-hover:opacity-70" />

						{/* Icon */}
						<div className="relative flex h-12 w-12 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/[0.1] text-primary shadow-inner">
							<svg
								className="h-5 w-5"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.8"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
								<polyline points="14 2 14 8 20 8" />
								<path d="M9 13h6" />
								<path d="M9 17h3" />
							</svg>
						</div>

						<div className="relative flex-1">
							<div className="font-mono text-[9px] uppercase tracking-[0.22em] text-primary/80">
								ready
							</div>
							<div className="mt-1 text-[15px] font-medium tracking-tight text-foreground">
								Join a signing session
							</div>
							<div className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
								Coordinate with other signers and approve transactions on your hardware device.
							</div>
						</div>

						<svg
							className="relative h-4 w-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-1 group-hover:text-foreground"
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
					</button>
				</div>

				{/* Footer micro-stats */}
				<div className="mt-8 flex items-center gap-6 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground/60">
					<div className="flex items-center gap-2">
						<span className="relative flex h-1.5 w-1.5">
							<span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-60" />
							<span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-success" />
						</span>
						Coordinator online
					</div>
					<span className="h-1 w-1 rounded-full bg-muted-foreground/30" />
					<div>regtest · testing network</div>
				</div>
			</section>

			{/* ═══ Right column: resources rail ═══ */}
			<aside className="relative w-[380px] border-l border-border/80 bg-card/40 px-8 py-10 backdrop-blur-sm">
				<div className="pointer-events-none absolute inset-y-0 left-0 w-px bg-gradient-to-b from-transparent via-border to-transparent" />

				<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					<span className="h-px w-6 bg-accent/60" />§ Resources
				</div>

				<h2 className="mt-3 text-[18px] font-medium tracking-tight text-foreground">
					Open in browser
				</h2>
				<p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
					Manage wallets, devices, and policies on the web dashboard.
				</p>

				<div className="mt-6 grid gap-px overflow-hidden rounded-md border border-border bg-border">
					{resources.map((item) => (
						<button
							key={item.key}
							type="button"
							onClick={() =>
								item.url.startsWith("http")
									? openUrl(item.url)
									: openUrl(`${WEBAPP_URL}${item.url}`)
							}
							className="group flex items-center gap-3 bg-background px-4 py-3.5 text-left transition-colors hover:bg-muted/60"
						>
							<div className="flex-1 min-w-0">
								<div className="text-[12.5px] font-medium text-foreground">{item.label}</div>
								<div className="mt-0.5 font-mono text-[9.5px] uppercase tracking-[0.14em] text-muted-foreground/70">
									{item.hint}
								</div>
							</div>
							<svg
								className="h-3.5 w-3.5 shrink-0 text-muted-foreground/40 transition-all group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-foreground"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.6"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M7 17 17 7" />
								<path d="M7 7h10v10" />
							</svg>
						</button>
					))}
				</div>

				<div className="mt-6 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground/50">
					opens {WEBAPP_URL.replace(/^https?:\/\//, "")}
				</div>
			</aside>
		</div>
	);
}
