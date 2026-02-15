// Session details page

import { useCallback, useEffect, useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { TransactionSigningData } from "../types/transaction";
import PageLayout from "../components/PageLayout";
import DeviceCreationSession from "../components/DeviceCreationSession";
import TransactionSigning from "../components/TransactionSigning";

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

	const handleDeviceSubmitted = () => {
		// Device registration completed - the session state will update via WebSocket
		console.log("Device registration submitted");
	};

	const handleSignatureSubmitted = () => {
		// Transaction signature completed - the session state will update via WebSocket
		console.log("Transaction signature submitted");
	};

	const sessionType = activeSession.sessionState?.sessionType;
	const isDeviceCreationSession = sessionType === "DEVICE_REGISTRATION";
	const isTransactionSigningSession = sessionType === "TRANSACTION_SIGNING";

	const network =
		activeSession.sessionState?.requirements?.network || "testnet";
	const derivationPath =
		activeSession.sessionState?.requirements?.derivation_path || "m/84'/0'/0'";

	// Extract transaction signing data from session requirements/data
	const transactionSigningData: TransactionSigningData | null =
		isTransactionSigningSession && activeSession.sessionState?.data
			? (activeSession.sessionState.data as TransactionSigningData)
			: null;

	console.log("Transaction Signing Data:", transactionSigningData);
	// Determine which content panel to show
	const renderSessionContent = () => {
		if (!activeSession.isConnected) {
			return null;
		}

		if (isDeviceCreationSession) {
			return (
				<DeviceCreationSession
					network={network}
					derivationPath={derivationPath}
					sessionId={activeSession.sessionId || ""}
					onDeviceSubmitted={handleDeviceSubmitted}
				/>
			);
		}

		if (isTransactionSigningSession && transactionSigningData) {
			return (
				<TransactionSigning
					transactionData={transactionSigningData}
					sessionId={activeSession.sessionId || ""}
					onSignatureSubmitted={handleSignatureSubmitted}
				/>
			);
		}

		// Default: show input panel for other session types
		return (
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
		);
	};

	return (
		<div className="page session-details-page">
			<PageLayout
				title="Session Details"
				showBackButton={true}
				backRoute="RemoteSessions"
			>
				<div className="session-header">
					<button type="button" onClick={handleExit} className="btn-exit">
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

						{activeSession.sessionState.requirements &&
							!isDeviceCreationSession &&
							!isTransactionSigningSession && (
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

				{renderSessionContent()}
			</PageLayout>
		</div>
	);
}
