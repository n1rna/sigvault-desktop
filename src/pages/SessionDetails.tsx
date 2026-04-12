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
		} catch {
			// Exit handled by backend
		}
	}, [clearActivityLog]);

	useEffect(() => {
		return () => { clearActivityLog(); };
	}, [clearActivityLog]);

	useEffect(() => {
		const handleBeforeUnload = () => {
			invoke("cmd_exit_session").catch(() => {});
		};

		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => {
			window.removeEventListener("beforeunload", handleBeforeUnload);
		};
	}, []);

	const handleDeviceSubmitted = () => {
		// Device registration completed
	};

	const handleSignatureSubmitted = () => {
		// Transaction signature completed
	};

	const sessionType = activeSession.sessionState?.sessionType;
	const isDeviceCreationSession = sessionType === "DEVICE_REGISTRATION";
	const isTransactionSigningSession = sessionType === "TRANSACTION_SIGNING";

	const network =
		(activeSession.sessionState?.requirements?.network as string) || "testnet";
	const derivationPath =
		(activeSession.sessionState?.requirements?.derivation_path as string) || "m/84'/0'/0'";

	const transactionSigningData: TransactionSigningData | null =
		isTransactionSigningSession && activeSession.sessionState?.data
			? (activeSession.sessionState.data as unknown as TransactionSigningData)
			: null;

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
					<div className="grid gap-px overflow-hidden rounded-lg bg-border">
						<div className="h-48 animate-pulse bg-card" />
						<div className="h-64 animate-pulse bg-card" />
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
		<div className="flex h-full w-full flex-col p-8">
			<div className="mb-4 flex shrink-0 items-center justify-between">
				<div>
					<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
						§ Active Session
					</div>
					<h1 className="mt-2 text-2xl font-medium tracking-tight text-foreground">
						Session Details
					</h1>
				</div>
				<button
					type="button"
					onClick={handleExit}
					className="rounded-md bg-destructive px-5 py-2.5 text-sm font-medium text-destructive-foreground transition-opacity hover:opacity-90"
				>
					Exit Session
				</button>
			</div>

			<div className="mb-6 flex items-center gap-3">
				<span
					className={`h-2.5 w-2.5 shrink-0 rounded-full ${
						activeSession.isConnected
							? "bg-success"
							: "bg-destructive"
					}`}
				/>
				{activeSession.sessionId ? (
					<span className="font-mono text-sm tabular-nums text-muted-foreground">
						{activeSession.sessionId}
					</span>
				) : (
					<span className="h-4 w-48 animate-pulse rounded bg-muted" />
				)}
				{sessionType ? (
					<span className="rounded-md bg-secondary px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-secondary-foreground">
						{sessionType}
					</span>
				) : (
					activeSession.sessionId && (
						<span className="h-5 w-28 animate-pulse rounded bg-muted" />
					)
				)}
			</div>

			<div className="min-h-0 flex-1 overflow-y-auto pb-8">
				{activeSession.sessionState?.error && (
					<div className="mb-6 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
						{activeSession.sessionState.error}
					</div>
				)}

				{!activeSession.isConnected && !activeSession.sessionState ? (
					<div className="grid gap-px overflow-hidden rounded-lg bg-border">
						<div className="h-48 animate-pulse bg-card" />
						<div className="h-64 animate-pulse bg-card" />
					</div>
				) : (
					renderSessionContent()
				)}
			</div>
		</div>
	);
}
