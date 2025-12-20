// Dashboard/Main page

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl as openExternal } from "@tauri-apps/plugin-opener";

interface UserInfo {
	id?: string;
	email?: string;
	name?: string;
	username?: string;
}

export default function Dashboard() {
	const [userInfo, setUserInfo] = useState<UserInfo | null>(null);
	const [loading, setLoading] = useState(true);

	useEffect(() => {
		const fetchUserInfo = async () => {
			try {
				const user = await invoke<UserInfo>("cmd_get_current_user");
				setUserInfo(user);
			} catch (error) {
				console.error("Failed to fetch user info:", error);
			} finally {
				setLoading(false);
			}
		};

		fetchUserInfo();
	}, []);

	const navigateToRemoteSessions = async () => {
		try {
			await invoke("cmd_navigate", { route: "RemoteSessions" });
		} catch (error) {
			console.error("Failed to navigate to remote sessions:", error);
		}
	};

	const openUrl = async (url: string) => {
		try {
			await openExternal(url);
		} catch (error) {
			console.error("Failed to open URL:", error);
		}
	};

	const getDisplayName = () => {
		if (userInfo?.name) return userInfo.name;
		if (userInfo?.username) return userInfo.username;
		if (userInfo?.email) return userInfo.email;
		return "User";
	};

	return (
		<div className="page dashboard-page">
			<div className="dashboard-container">
				<div className="dashboard-welcome">
					<h1>Welcome, {loading ? "..." : getDisplayName()}</h1>
				</div>

				<div className="dashboard-links">
					<button
						type="button"
						className="dashboard-link"
						onClick={navigateToRemoteSessions}
					>
						<span>Remote Sessions</span>
					</button>

					<button
						type="button"
						className="dashboard-link external"
						onClick={() => openUrl("https://docs.sigvault.com")}
					>
						<span>Documentation</span>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
							<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
							<polyline points="15 3 21 3 21 9" />
							<line x1="10" y1="14" x2="21" y2="3" />
						</svg>
					</button>

					<button
						type="button"
						className="dashboard-link external"
						onClick={() => openUrl("https://app.sigvault.com/dash/settings")}
					>
						<span>Profile Settings</span>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
							<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
							<polyline points="15 3 21 3 21 9" />
							<line x1="10" y1="14" x2="21" y2="3" />
						</svg>
					</button>
				</div>
			</div>
		</div>
	);
}
