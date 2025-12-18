// Session details page

import { useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import PageLayout from "../components/PageLayout";

export default function SessionDetails() {
	const { activeSession } = useAppState();
	const [userInput, setUserInput] = useState("");
	const [submitting, setSubmitting] = useState(false);

	const handleExit = async () => {
		try {
			await invoke<CommandResult>("cmd_exit_session");
		} catch (error) {
			console.error("Failed to exit session:", error);
		}
	};

	const handleSubmitInput = async (e: React.FormEvent) => {
		e.preventDefault();
		if (!userInput.trim()) return;

		setSubmitting(true);
		try {
			await invoke<CommandResult>("cmd_submit_user_input_session_websocket", {
				sessionId: activeSession.sessionId || "",
				input: userInput,
			});
			setUserInput("");
		} catch (error) {
			console.error("Failed to submit input:", error);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div className="page session-details-page">
			<PageLayout
				title="Session Details"
				showBackButton={true}
				backRoute="RemoteSessions"
			>
				<div className="session-header">
					<button onClick={handleExit} className="btn-exit">
						Exit Session
					</button>
				</div>

				<div className="session-info-panel">
					<div className="info-row">
						<span className="label">Session ID:</span>
						<span className="value">{activeSession.sessionId || "N/A"}</span>
					</div>
					<div className="info-row">
						<span className="label">Type:</span>
						<span className="value">{activeSession.sessionType || "N/A"}</span>
					</div>
					<div className="info-row">
						<span className="label">Status:</span>
						<span
							className={`status ${activeSession.isConnected ? "connected" : "disconnected"}`}
						>
							{activeSession.isConnected ? "Connected" : "Disconnected"}
						</span>
					</div>
				</div>

				{activeSession.sessionState && (
					<div className="session-state-panel">
						<h2>Current Step: {activeSession.sessionState.step || "N/A"}</h2>

						{activeSession.sessionState.error && (
							<div className="error-message">
								{activeSession.sessionState.error}
							</div>
						)}

						{activeSession.sessionState.requirements && (
							<div className="requirements-panel">
								<h3>Requirements:</h3>
								<pre>
									{JSON.stringify(
										activeSession.sessionState.requirements,
										null,
										2,
									)}
								</pre>
							</div>
						)}
					</div>
				)}

				{activeSession.isConnected && (
					<div className="input-panel">
						<h3>Submit Input</h3>
						<form onSubmit={handleSubmitInput}>
							<textarea
								value={userInput}
								onChange={(e) => setUserInput(e.target.value)}
								placeholder="Enter input data..."
								rows={4}
								disabled={submitting}
							/>
							<button type="submit" disabled={submitting || !userInput.trim()}>
								{submitting ? "Submitting..." : "Submit"}
							</button>
						</form>
					</div>
				)}
			</PageLayout>
		</div>
	);
}
