import { useCallback, useEffect } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";
import type { TransactionSigningData } from "../types/transaction";
import DeviceCreationSession from "../components/DeviceCreationSession";
import TransactionSigning from "../components/TransactionSigning";

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

	const friendlyType = sessionType
		? sessionType.replace(/_/g, " ").toLowerCase()
		: null;

	return (
		<div className="relative flex h-full w-full flex-col overflow-hidden">
			{/* ── Ambient background ── */}
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.05]" />

			{/* ── Header ── */}
			<div className="relative shrink-0 px-12 pt-10 pb-6">
				<div className="flex items-start justify-between gap-6">
					<div className="flex-1">
						<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							<span className="h-px w-6 bg-primary/60" />
							§ Active session
						</div>
						<h1 className="mt-4 text-[32px] font-medium leading-[1.1] tracking-[-0.02em] text-foreground">
							{friendlyType
								? friendlyType.replace(/^\w/, (c) => c.toUpperCase())
								: "Session details"}
						</h1>
					</div>

					<button
						type="button"
						onClick={handleExit}
						className="flex h-10 items-center gap-2 rounded-md border border-destructive/30 bg-destructive/[0.06] px-4 font-mono text-[10px] uppercase tracking-[0.18em] text-destructive transition-colors hover:border-destructive/60 hover:bg-destructive/[0.12]"
					>
						<svg
							className="h-3.5 w-3.5"
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
					</button>
				</div>

				{/* Meta chips */}
				<div className="mt-5 flex flex-wrap items-center gap-2">
					{/* Connection status */}
					<div
						className={`flex h-8 items-center gap-2 rounded-full border px-3 font-mono text-[10px] uppercase tracking-[0.16em] ${
							activeSession.isConnected
								? "border-success/30 bg-success/[0.06] text-success"
								: "border-destructive/30 bg-destructive/[0.06] text-destructive"
						}`}
					>
						<span className="relative flex h-1.5 w-1.5">
							{activeSession.isConnected && (
								<span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-60" />
							)}
							<span
								className={`relative inline-flex h-1.5 w-1.5 rounded-full ${
									activeSession.isConnected
										? "bg-success"
										: "bg-destructive"
								}`}
							/>
						</span>
						{activeSession.isConnected ? "connected" : "disconnected"}
					</div>

					{/* Session type */}
					{sessionType ? (
						<div className="flex h-8 items-center gap-2 rounded-full border border-primary/30 bg-primary/[0.06] px-3 font-mono text-[10px] uppercase tracking-[0.16em]">
							<span className="text-muted-foreground">type</span>
							<span className="text-foreground">
								{sessionType}
							</span>
						</div>
					) : (
						activeSession.sessionId && (
							<span className="h-8 w-36 animate-pulse rounded-full bg-muted/40" />
						)
					)}

					{/* Session ID */}
					{activeSession.sessionId ? (
						<div className="flex h-8 items-center gap-2 rounded-full border border-border bg-card/60 px-3 font-mono text-[10px] uppercase tracking-[0.16em]">
							<span className="text-muted-foreground">id</span>
							<span className="font-mono text-[11px] normal-case tracking-normal tabular-nums text-foreground">
								{shortId(activeSession.sessionId)}
							</span>
						</div>
					) : (
						<span className="h-8 w-48 animate-pulse rounded-full bg-muted/40" />
					)}
				</div>
			</div>

			{/* ── Content ── */}
			<div className="relative min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-12 pb-10">
				{activeSession.sessionState?.error && (
					<div className="mb-6 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
						<svg className="mt-0.5 h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
							<circle cx="12" cy="12" r="10" />
							<line x1="12" y1="8" x2="12" y2="12" />
							<line x1="12" y1="16" x2="12.01" y2="16" />
						</svg>
						<span className="leading-snug">
							{activeSession.sessionState.error}
						</span>
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
	);
}
