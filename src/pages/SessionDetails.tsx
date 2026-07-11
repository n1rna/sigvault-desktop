import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect } from "react";
import DeviceCreationSession from "../components/DeviceCreationSession";
import TransactionSigning from "../components/TransactionSigning";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { useAppState } from "../contexts/AppStateContext";
import type { CommandResult } from "../types/events";
import type { TransactionSigningData } from "../types/transaction";

function shortId(id: string) {
	if (id.length <= 14) return id;
	return `${id.slice(0, 6)}…${id.slice(-6)}`;
}

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
		return () => {
			clearActivityLog();
		};
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

	const network = (activeSession.sessionState?.requirements?.network as string) || "testnet";
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
					<div className="grid gap-px overflow-hidden rounded-lg border border-border bg-border">
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

	const friendlyType = sessionType ? sessionType.replace(/_/g, " ").toLowerCase() : null;

	return (
		<div className="flex h-full w-full flex-col overflow-hidden">
			{/* ── Header ── */}
			<div className="shrink-0 border-b border-border px-8 py-4">
				<div className="mx-auto flex max-w-3xl items-start justify-between gap-6">
					<div className="flex-1">
						<h1 className="text-[20px] font-semibold tracking-tight text-foreground">
							{friendlyType
								? friendlyType.replace(/^\w/, (c) => c.toUpperCase())
								: "Session details"}
						</h1>

						{/* Meta chips */}
						<div className="mt-3 flex flex-wrap items-center gap-2">
							<Badge variant={activeSession.isConnected ? "success" : "destructive"}>
								<span
									className={`h-1.5 w-1.5 rounded-full ${
										activeSession.isConnected ? "bg-success" : "bg-destructive"
									}`}
								/>
								{activeSession.isConnected ? "Connected" : "Disconnected"}
							</Badge>

							{sessionType ? (
								<Badge variant="secondary">{sessionType}</Badge>
							) : (
								activeSession.sessionId && (
									<span className="h-6 w-28 animate-pulse rounded-full bg-muted/40" />
								)
							)}

							{activeSession.sessionId ? (
								<span className="text-[12px] text-muted-foreground">
									<span className="font-mono tabular-nums">{shortId(activeSession.sessionId)}</span>
								</span>
							) : (
								<span className="h-6 w-40 animate-pulse rounded-full bg-muted/40" />
							)}
						</div>
					</div>

					<Button variant="outline" size="sm" onClick={handleExit}>
						<svg
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<path d="M18 6 6 18" />
							<path d="m6 6 12 12" />
						</svg>
						Exit session
					</Button>
				</div>
			</div>

			{/* ── Content ── */}
			<div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-8 py-8">
				<div className="mx-auto max-w-3xl">
					{activeSession.sessionState?.error && (
						<div className="mb-6 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
							<svg
								className="mt-0.5 h-3.5 w-3.5 shrink-0"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<circle cx="12" cy="12" r="10" />
								<line x1="12" y1="8" x2="12" y2="12" />
								<line x1="12" y1="16" x2="12.01" y2="16" />
							</svg>
							<span className="leading-snug">{activeSession.sessionState.error}</span>
						</div>
					)}

					{!activeSession.isConnected && !activeSession.sessionState ? (
						<div className="grid gap-px overflow-hidden rounded-lg border border-border bg-border">
							<div className="h-48 animate-pulse bg-card" />
							<div className="h-64 animate-pulse bg-card" />
						</div>
					) : (
						renderSessionContent()
					)}
				</div>
			</div>
		</div>
	);
}
