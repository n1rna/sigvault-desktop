// Dashboard/Main page

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppState } from "../contexts/AppStateContext";

interface UserInfo {
	id?: string;
	email?: string;
	name?: string;
	username?: string;
}

export default function Dashboard() {
	const { activeSession } = useAppState();
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

	const getDisplayName = () => {
		if (userInfo?.name) return userInfo.name;
		if (userInfo?.username) return userInfo.username;
		if (userInfo?.email) return userInfo.email;
		return "User";
	};

	return (
		<div className="page dashboard-page">
			<div className="dashboard-header">
				<h1>Welcome back, {loading ? "..." : getDisplayName()}!</h1>
				<p className="subtitle">Sigvault Desktop</p>
			</div>

			<div className="dashboard-content">
				<div className="user-info-card">
					<h2>Your Profile</h2>
					{loading ? (
						<p>Loading user information...</p>
					) : userInfo ? (
						<div className="user-details">
							{userInfo.name && (
								<div className="user-field">
									<span className="field-label">Name:</span>
									<span className="field-value">{userInfo.name}</span>
								</div>
							)}
							{userInfo.username && (
								<div className="user-field">
									<span className="field-label">Username:</span>
									<span className="field-value">{userInfo.username}</span>
								</div>
							)}
							{userInfo.email && (
								<div className="user-field">
									<span className="field-label">Email:</span>
									<span className="field-value">{userInfo.email}</span>
								</div>
							)}
						</div>
					) : (
						<p>Unable to load user information</p>
					)}
				</div>

				<div className="status-card">
					<h2>Connection Status</h2>
					<div className="status-indicator">
						<span
							className={`status-dot ${activeSession.isConnected ? "connected" : "disconnected"}`}
						></span>
						<span className="status-text">
							{activeSession.isConnected ? "Connected" : "Disconnected"}
						</span>
					</div>
				</div>

				<div className="actions-card">
					<h2>Quick Actions</h2>
					<button
						type="button"
						className="primary-button"
						onClick={navigateToRemoteSessions}
					>
						View Remote Sessions
					</button>
				</div>
			</div>
		</div>
	);
}
