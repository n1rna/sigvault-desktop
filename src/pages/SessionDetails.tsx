import { useCallback, useEffect } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { TransactionSigningData } from "../types/transaction";
import DeviceCreationSession from "../components/DeviceCreationSession";
import TransactionSigning from "../components/TransactionSigning";

export default function SessionDetails() {
	const { activeSession, clearActivityLog } = useAppState();

	const handleExit = useCallback(async () => {
		clearActivityLog();
		try {
			await invoke<CommandResult>("cmd_exit_session");
		} catch (error) {
			console.error("Failed to exit session:", error);
		}
	}, [clearActivityLog]);

	useEffect(() => {
		return () => { clearActivityLog(); };
	}, [clearActivityLog]);

	useEffect(() => {
		const handleBeforeUnload = () => {
			invoke("cmd_exit_session").catch(console.error);
		};

		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => {
			window.removeEventListener("beforeunload", handleBeforeUnload);
		};
	}, []);

	const handleDeviceSubmitted = () => {
		console.log("Device registration submitted");
	};

	const handleSignatureSubmitted = () => {
		console.log("Transaction signature submitted");
	};

	const sessionType = activeSession.sessionState?.sessionType;
	const isDeviceCreationSession = sessionType === "DEVICE_REGISTRATION";
	const isTransactionSigningSession = sessionType === "TRANSACTION_SIGNING";

	const network =
		activeSession.sessionState?.requirements?.network || "testnet";
	const derivationPath =
		activeSession.sessionState?.requirements?.derivation_path || "m/84'/0'/0'";

	const transactionSigningData: TransactionSigningData | null =
		isTransactionSigningSession && activeSession.sessionState?.data
			? (activeSession.sessionState.data as TransactionSigningData)
			: null;

	console.log("Transaction Signing Data:", transactionSigningData);

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

		if (isTransactionSigningSession) {
			if (!transactionSigningData) {
				return (
					<div className="flex flex-col gap-6">
						<div className="h-48 animate-pulse bg-muted" />
						<div className="h-64 animate-pulse bg-muted" />
					</div>
				);
			}
			return (
				<TransactionSigning
					transactionData={transactionSigningData}
					sessionId={activeSession.sessionId || ""}
					onSignatureSubmitted={handleSignatureSubmitted}
				/>
			);
		}

		return null;
	};

	return (
		<div className="flex h-full w-full flex-col overflow-hidden p-8">
			<div className="mb-4 flex shrink-0 items-center justify-between">
				<h1 className="text-[1.75rem] font-semibold text-foreground">
					Session Details
				</h1>
				<button
					type="button"
					onClick={handleExit}
					className="bg-destructive px-5 py-2.5 text-sm font-medium text-destructive-foreground hover:opacity-90"
				>
					Exit Session
				</button>
			</div>

			<div className="mb-6 flex items-center gap-3">
				<span
					className={`h-2.5 w-2.5 shrink-0 rounded-full ${
						activeSession.isConnected
							? "bg-green-500"
							: "bg-destructive"
					}`}
				/>
				{activeSession.sessionId ? (
					<span className="font-mono text-sm text-muted-foreground">
						{activeSession.sessionId}
					</span>
				) : (
					<span className="h-4 w-48 animate-pulse bg-muted" />
				)}
				{sessionType ? (
					<span className="border border-border bg-secondary px-2 py-0.5 text-xs text-secondary-foreground">
						{sessionType}
					</span>
				) : (
					activeSession.sessionId && (
						<span className="h-5 w-28 animate-pulse bg-muted" />
					)
				)}
			</div>

			<div className="flex-1 overflow-y-auto overflow-x-hidden">
				{activeSession.sessionState?.error && (
					<div className="mb-6 border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
						{activeSession.sessionState.error}
					</div>
				)}

				{!activeSession.isConnected && !activeSession.sessionState ? (
					<div className="flex flex-col gap-6">
						<div className="h-48 animate-pulse bg-muted" />
						<div className="h-64 animate-pulse bg-muted" />
					</div>
				) : (
					renderSessionContent()
				)}
			</div>
		</div>
	);
}
