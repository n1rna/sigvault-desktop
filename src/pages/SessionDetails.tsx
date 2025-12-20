// Session details page

import { useCallback, useEffect, useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { DeviceInfo } from "../types/hardware";
import PageLayout from "../components/PageLayout";
import DeviceDiscovery from "../components/DeviceDiscovery";

export default function SessionDetails() {
	const { activeSession } = useAppState();
	const [userInput, setUserInput] = useState("");
	const [submitting, setSubmitting] = useState(false);

	const handleExit = useCallback(async () => {
		try {
			await invoke<CommandResult>("cmd_exit_session");
		} catch (error) {
			console.error("Failed to exit session:", error);
		}
	}, []);

	useEffect(() => {
		const handleBeforeUnload = () => {
			invoke("cmd_exit_session").catch(console.error);
		};

		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => {
			window.removeEventListener("beforeunload", handleBeforeUnload);
		};
	}, []);

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

	const handleDeviceSelected = async (deviceInfo: DeviceInfo) => {
		setSubmitting(true);
		try {
			await invoke<CommandResult>("cmd_submit_device_registration", {
				sessionId: activeSession.sessionId || "",
				deviceInfo: {
					xpub: deviceInfo.xpub,
					fingerprint: deviceInfo.fingerprint,
					derivation_path: deviceInfo.derivation_path,
					device_type: deviceInfo.device_type,
				},
			});
		} catch (error) {
			console.error("Failed to submit device registration:", error);
		} finally {
			setSubmitting(false);
		}
	};

	const isDeviceCreationSession =
		activeSession.sessionState?.sessionType === "DEVICE_REGISTRATION";
	const network =
		activeSession.sessionState?.requirements?.network || "testnet";
	const derivationPath = "m/48'/0'/0'/2'";

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
						<span className="value">
							{activeSession.sessionState?.sessionType || "N/A"}
						</span>
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

				{activeSession.isConnected && isDeviceCreationSession && (
					<DeviceDiscovery
						network={network}
						derivationPath={derivationPath}
						onDeviceSelected={handleDeviceSelected}
					/>
				)}

				{activeSession.isConnected && !isDeviceCreationSession && (
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
