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
		<div className="h-full overflow-hidden p-8">
			<div className="mx-auto flex h-full max-w-[500px] flex-col justify-center">
				<div className="mb-12 text-center">
					<h1 className="text-2xl font-medium text-foreground">
						Welcome, {loading ? "..." : getDisplayName()}
					</h1>
				</div>

				<div className="flex flex-col gap-3">
					<button
						type="button"
						className="flex items-center justify-between bg-transparent px-4 py-3 text-left text-base text-foreground hover:bg-card"
						onClick={navigateToRemoteSessions}
					>
						<span>Remote Sessions</span>
					</button>

					<button
						type="button"
						className="flex items-center justify-between bg-transparent px-4 py-3 text-left text-base text-foreground hover:bg-card"
						onClick={() => openUrl("https://docs.sigvault.com")}
					>
						<span>Documentation</span>
						<svg
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							aria-hidden="true"
							className="shrink-0 text-muted-foreground"
						>
							<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
							<polyline points="15 3 21 3 21 9" />
							<line x1="10" y1="14" x2="21" y2="3" />
						</svg>
					</button>

					<button
						type="button"
						className="flex items-center justify-between bg-transparent px-4 py-3 text-left text-base text-foreground hover:bg-card"
						onClick={() =>
							openUrl("https://app.sigvault.com/dash/settings")
						}
					>
						<span>Profile Settings</span>
						<svg
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							aria-hidden="true"
							className="shrink-0 text-muted-foreground"
						>
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
