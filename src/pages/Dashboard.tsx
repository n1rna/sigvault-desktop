import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl as openExternal } from "@tauri-apps/plugin-opener";

interface UserInfo {
	id?: string;
	email?: string;
	name?: string;
	username?: string;
}

const WEBAPP_URL = import.meta.env.VITE_WEBAPP_URL || "https://app.sigvault.org";

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

	const getInitials = () => {
		const name = getDisplayName();
		return name.split(" ").map((w) => w[0]).join("").toUpperCase().slice(0, 2);
	};

	return (
		<div className="flex h-full flex-col p-6">
			{/* Header with eyebrow */}
			<div className="mb-6">
				<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Overview
				</div>
				<div className="mt-3 flex items-center gap-3">
					<div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary/15 font-mono text-xs font-semibold text-primary">
						{loading ? ".." : getInitials()}
					</div>
					<div>
						<h1 className="text-lg font-medium tracking-tight text-foreground">
							{loading ? "Loading…" : `Welcome, ${getDisplayName()}`}
						</h1>
						{userInfo?.email && (
							<p className="font-mono text-xs text-muted-foreground">{userInfo.email}</p>
						)}
					</div>
				</div>
			</div>

			<div className="mb-5">
				<div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Quick Actions
				</div>
				<div className="grid grid-cols-4 gap-px overflow-hidden rounded-lg border border-border bg-border">
					{[
						{
							label: "Sessions",
							desc: "Connect to remote sessions",
							action: () => navigateTo("RemoteSessions"),
							icon: <><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" /><polyline points="10 17 15 12 10 7" /><line x1="15" y1="12" x2="3" y2="12" /></>,
						},
						{
							label: "Dashboard",
							desc: "Open web dashboard",
							action: () => openUrl(`${WEBAPP_URL}/dash`),
							icon: <><rect x="3" y="3" width="7" height="7" /><rect x="14" y="3" width="7" height="7" /><rect x="3" y="14" width="7" height="7" /><rect x="14" y="14" width="7" height="7" /></>,
							external: true,
						},
						{
							label: "Wallets",
							desc: "Manage your wallets",
							action: () => openUrl(`${WEBAPP_URL}/dash/wallets`),
							icon: <><path d="M21 12V7H5a2 2 0 0 1 0-4h14v4" /><path d="M3 5v14a2 2 0 0 0 2 2h16v-5" /><path d="M18 12a2 2 0 0 0 0 4h4v-4Z" /></>,
							external: true,
						},
						{
							label: "Devices",
							desc: "Manage hardware devices",
							action: () => openUrl(`${WEBAPP_URL}/dash/devices`),
							icon: <><rect x="2" y="7" width="20" height="14" rx="2" ry="2" /><path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16" /></>,
							external: true,
						},
					].map((item) => (
						<button
							key={item.label}
							type="button"
							onClick={item.action}
							className="flex flex-col items-center gap-2 bg-card px-3 py-4 text-center transition-colors hover:bg-muted"
						>
							<svg className="h-5 w-5 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
								{item.icon}
							</svg>
							<span className="text-xs font-medium text-foreground">{item.label}</span>
						</button>
					))}
				</div>
			</div>

			<div className="flex-1">
				<div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Resources
				</div>
				<div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-border bg-border">
					{[
						{ label: "Documentation", url: `${WEBAPP_URL}/docs`, icon: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" },
						{ label: "Account Settings", url: `${WEBAPP_URL}/dash/settings`, icon: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" },
						{ label: "API Keys", url: `${WEBAPP_URL}/dash/settings/api-keys`, icon: "m21 2-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4" },
						{ label: "GitHub", url: "https://github.com/n1rna/sigvault", icon: "M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22" },
					].map((item) => (
						<button
							key={item.label}
							type="button"
							onClick={() => openUrl(item.url)}
							className="flex items-center gap-3 bg-card px-4 py-3 text-left transition-colors hover:bg-muted"
						>
							<svg className="h-4 w-4 shrink-0 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
								<path d={item.icon} />
							</svg>
							<span className="flex-1 text-sm text-foreground">{item.label}</span>
							<svg className="h-3 w-3 shrink-0 text-muted-foreground/40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
								<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
								<polyline points="15 3 21 3 21 9" />
								<line x1="10" y1="14" x2="21" y2="3" />
							</svg>
						</button>
					))}
				</div>
			</div>

		</div>
	);
}
