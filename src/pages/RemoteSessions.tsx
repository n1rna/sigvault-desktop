// Remote sessions list page

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult, RemoteSession } from "../types/events";
import PageLayout from "../components/PageLayout";
import { useAppState } from "../contexts/AppStateContext";

export default function RemoteSessions() {
	useAppState();
	const [remoteSessions, setRemoteSessions] = useState<RemoteSession[]>([]);
	const [loading, setLoading] = useState(true);
	const [connectingTo, setConnectingTo] = useState<string | null>(null);

	const loadSessions = async () => {
		try {
			const sessions = await invoke<RemoteSession[]>("cmd_get_remote_sessions");
			setRemoteSessions(sessions);
		} catch (error) {
			console.error("Failed to load sessions:", error);
		} finally {
			setLoading(false);
		}
	};

	const fetchSessions = async () => {
		setLoading(true);
		try {
			await invoke<CommandResult>("cmd_update_remote_sessions");
			// After updating, reload from app state
			await loadSessions();
		} catch (error) {
			console.error("Failed to fetch sessions:", error);
			setLoading(false);
		}
	};

	const handleConnect = async (sessionId: string) => {
		setConnectingTo(sessionId);
		try {
			const result = await invoke<CommandResult>(
				"cmd_start_session_websocket_connection",
				{
					sessionId,
				},
			);

			if (!result.success) {
				console.log("Failed to connect:", result.message);
			}
		} catch (error) {
			console.log("Failed to connect to session:", error);
		} finally {
			setConnectingTo(null);
		}
	};

	useEffect(() => {
		// Fetch sessions from app state on mount
		fetchSessions();
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	return (
		<div className="page sessions-page">
			<PageLayout title="Remote Sessions" showBackButton={true}>
				<div className="sessions-header">
					<button type="button" onClick={fetchSessions} disabled={loading}>
						{loading ? "Refreshing..." : "Refresh"}
					</button>
				</div>

				<div className="sessions-list">
					{loading && remoteSessions.length === 0 ? (
						<p className="sessions-empty">Loading sessions...</p>
					) : remoteSessions.length === 0 ? (
						<p className="sessions-empty">No sessions available</p>
					) : (
						remoteSessions.map((session) => (
							<div key={session.id} className="session-card">
								<div className="session-info">
									<h3>{session.session_type}</h3>
									<p className="session-id">ID: {session.id}</p>
									<span
										className={`session-status status-${session.status.toLowerCase()}`}
									>
										{session.status}
									</span>
								</div>
								<button
									type="button"
									onClick={() => handleConnect(session.id)}
									disabled={connectingTo === session.id}
									className="btn-connect"
								>
									{connectingTo === session.id ? "Connecting..." : "Connect"}
								</button>
							</div>
						))
					)}
				</div>
			</PageLayout>
		</div>
	);
}
