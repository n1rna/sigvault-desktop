import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl as openExternal } from "@tauri-apps/plugin-opener";

interface UserInfo {
	id?: string;
	email?: string;
	name?: string;
	username?: string;
}

const WEBAPP_URL = import.meta.env.VITE_WEBAPP_URL || "https://regtest.sigvault.org";

const resources = [
	{ label: "Web Dashboard", url: "/dash", key: "dash" },
	{ label: "Wallets", url: "/dash/wallets", key: "wallets" },
	{ label: "Devices", url: "/dash/devices", key: "devices" },
	{ label: "Documentation", url: "/docs", key: "docs" },
	{ label: "Account Settings", url: "/dash/settings", key: "settings" },
	{ label: "API Keys", url: "/dash/settings/api-keys", key: "keys" },
];

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
		if (userInfo?.email) return userInfo.email;
		return "User";
	};

	return (
		<div className="relative flex h-full flex-col">
			{/* Ambient background */}
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.06]" />

			{/* Main content — vertically centered */}
			<div className="relative flex flex-1 flex-col items-center justify-center px-8">
				{/* User greeting */}
				<div className="mb-10 text-center">
					<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground/60">
						§ Overview
					</div>
					<h1 className="mt-3 text-2xl font-medium tracking-tight text-foreground">
						{loading ? "Loading…" : getDisplayName()}
					</h1>
					{!loading && userInfo?.email && (
						<p className="mt-1 font-mono text-xs text-muted-foreground/50">
							{userInfo.email}
						</p>
					)}
				</div>

				{/* Sessions CTA */}
				<button
					type="button"
					onClick={() => navigateTo("RemoteSessions")}
					className="group relative mb-12 overflow-hidden rounded-lg border border-primary/20 bg-primary/[0.06] px-10 py-4 transition-all hover:border-primary/40 hover:bg-primary/[0.1]"
				>
					<div className="pointer-events-none absolute inset-0 bg-grid-sm opacity-[0.04]" />
					<div className="relative flex items-center gap-3">
						<span className="flex h-2 w-2 rounded-full bg-primary/80 shadow-[0_0_6px_1px] shadow-primary/30" />
						<span className="font-mono text-sm font-medium tracking-wide text-foreground">
							Open Sessions
						</span>
						<svg className="ml-1 h-3.5 w-3.5 text-muted-foreground transition-transform group-hover:translate-x-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
							<path d="m9 18 6-6-6-6" />
						</svg>
					</div>
				</button>

				{/* Resources list */}
				<div className="w-full max-w-xs">
					<div className="mb-2 text-center font-mono text-[9px] uppercase tracking-[0.3em] text-muted-foreground/40">
						Resources
					</div>
					<div className="rounded-md border border-border/60">
						{resources.map((item, i) => (
							<button
								key={item.key}
								type="button"
								onClick={() =>
									item.url.startsWith("http")
										? openUrl(item.url)
										: openUrl(`${WEBAPP_URL}${item.url}`)
								}
								className={`flex w-full items-center justify-between px-4 py-2.5 text-left transition-colors hover:bg-muted/50 ${
									i > 0 ? "border-t border-border/40" : ""
								}`}
							>
								<span className="text-xs text-muted-foreground transition-colors group-hover:text-foreground">
									{item.label}
								</span>
								<svg className="h-3 w-3 text-muted-foreground/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
									<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
									<polyline points="15 3 21 3 21 9" />
									<line x1="10" y1="14" x2="21" y2="3" />
								</svg>
							</button>
						))}
					</div>
				</div>
			</div>
		</div>
	);
}
