import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import type { CommandResult, RemoteSession } from "../types/events";

type SortOrder = "newest" | "oldest";

function shortId(id: string) {
	if (id.length <= 14) return id;
	return `${id.slice(0, 6)}…${id.slice(-6)}`;
}

function formatRelative(iso?: string) {
	if (!iso) return "—";
	const then = new Date(iso).getTime();
	const now = Date.now();
	const diff = Math.max(0, now - then);
	const mins = Math.floor(diff / 60000);
	if (mins < 1) return "just now";
	if (mins < 60) return `${mins}m ago`;
	const hrs = Math.floor(mins / 60);
	if (hrs < 24) return `${hrs}h ago`;
	const days = Math.floor(hrs / 24);
	if (days < 7) return `${days}d ago`;
	return new Date(iso).toLocaleDateString();
}

export default function RemoteSessions() {
	useAppState();
	const [remoteSessions, setRemoteSessions] = useState<RemoteSession[]>([]);
	const [loading, setLoading] = useState(true);
	const [connectingTo, setConnectingTo] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [typeFilter, setTypeFilter] = useState<string>("all");
	const [sortOrder, setSortOrder] = useState<SortOrder>("newest");

	const loadSessions = async () => {
		try {
			const sessions = await invoke<RemoteSession[]>("cmd_get_remote_sessions");
			setRemoteSessions(sessions);
		} catch {
			// Session loading errors are non-fatal
		} finally {
			setLoading(false);
		}
	};

	const fetchSessions = async () => {
		setLoading(true);
		try {
			await invoke<CommandResult>("cmd_update_remote_sessions");
			await loadSessions();
		} catch {
			setLoading(false);
		}
	};

	const handleConnect = async (sessionId: string) => {
		setConnectingTo(sessionId);
		try {
			const result = await invoke<CommandResult>("cmd_start_session_websocket_connection", {
				sessionId,
			});
			if (!result.success) {
				setError(result.message || "Failed to connect");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to connect to session");
		} finally {
			setConnectingTo(null);
		}
	};

	useEffect(() => {
		fetchSessions();
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	const sessionTypes = useMemo(() => {
		const types = new Set(remoteSessions.map((s) => s.session_type));
		return Array.from(types).sort();
	}, [remoteSessions]);

	const filteredAndSorted = useMemo(() => {
		let result = remoteSessions.map((s, i) => ({ ...s, _index: i }));

		if (typeFilter !== "all") {
			result = result.filter((s) => s.session_type === typeFilter);
		}

		const sorted = [...result].sort((a, b) => {
			if (a.created_at && b.created_at) {
				const diff = new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
				if (diff !== 0) return sortOrder === "newest" ? diff : -diff;
			}
			return sortOrder === "newest" ? b._index - a._index : a._index - b._index;
		});

		return sorted;
	}, [remoteSessions, typeFilter, sortOrder]);

	const activeCount = useMemo(
		() => remoteSessions.filter((s) => s.status.toLowerCase() === "active").length,
		[remoteSessions],
	);

	return (
		<div className="relative flex h-full w-full flex-col overflow-hidden">
			{/* ── Ambient background ── */}
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.05]" />

			{/* ── Header ── */}
			<div className="relative shrink-0 px-12 pt-10 pb-6">
				<div className="flex items-start justify-between gap-6">
					<div className="flex-1">
						<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
							<span className="h-px w-6 bg-primary/60" />§ Sessions
						</div>
						<h1 className="mt-4 text-[32px] font-medium leading-[1.1] tracking-[-0.02em] text-foreground">
							Remote Sessions
						</h1>
						<p className="mt-2 max-w-[520px] text-[13px] leading-relaxed text-muted-foreground">
							Join active signing ceremonies or wait for new ones. Connect with your hardware wallet
							to contribute a signature.
						</p>
					</div>

					{/* Stat chips + refresh */}
					<div className="flex items-center gap-3">
						<div className="flex items-center gap-2 rounded-md border border-border bg-card/60 px-3.5 py-2">
							<span className="font-mono text-[9px] uppercase tracking-[0.2em] text-muted-foreground">
								total
							</span>
							<span className="font-mono text-[15px] font-medium tabular-nums text-foreground">
								{remoteSessions.length}
							</span>
						</div>
						<div className="flex items-center gap-2 rounded-md border border-success/30 bg-success/[0.06] px-3.5 py-2">
							<span className="relative flex h-1.5 w-1.5">
								<span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-60" />
								<span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-success" />
							</span>
							<span className="font-mono text-[9px] uppercase tracking-[0.2em] text-success">
								active
							</span>
							<span className="font-mono text-[15px] font-medium tabular-nums text-foreground">
								{activeCount}
							</span>
						</div>
						<button
							type="button"
							onClick={fetchSessions}
							disabled={loading}
							className="flex h-10 w-10 items-center justify-center rounded-md border border-border bg-card/60 text-muted-foreground transition-colors hover:border-primary/40 hover:bg-primary/[0.06] hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
							title="Refresh sessions"
						>
							<svg
								className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.8"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
								<path d="M3 3v5h5" />
								<path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
								<path d="M16 16h5v5" />
							</svg>
						</button>
					</div>
				</div>

				{error && (
					<div className="mt-5 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
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
						<span className="leading-snug">{error}</span>
					</div>
				)}

				{/* Filter bar */}
				{remoteSessions.length > 0 && (
					<div className="mt-6 flex items-center justify-between gap-4">
						<div className="flex items-center gap-1.5">
							<span className="mr-2 font-mono text-[9px] uppercase tracking-[0.2em] text-muted-foreground/70">
								filter
							</span>
							{["all", ...sessionTypes].map((type) => {
								const isActive = typeFilter === type;
								return (
									<button
										key={type}
										type="button"
										onClick={() => setTypeFilter(type)}
										className={`flex h-7 items-center rounded-full border px-3 font-mono text-[10px] uppercase tracking-[0.14em] transition-colors ${
											isActive
												? "border-primary/50 bg-primary/[0.1] text-foreground"
												: "border-border/60 bg-transparent text-muted-foreground hover:border-border hover:text-foreground"
										}`}
									>
										{type === "all" ? "all" : type}
									</button>
								);
							})}
						</div>

						<div className="flex items-center gap-3">
							{typeFilter !== "all" && (
								<span className="font-mono text-[10px] uppercase tracking-[0.16em] tabular-nums text-muted-foreground">
									{filteredAndSorted.length} / {remoteSessions.length}
								</span>
							)}
							<button
								type="button"
								onClick={() => setSortOrder((s) => (s === "newest" ? "oldest" : "newest"))}
								className="flex h-7 items-center gap-1.5 rounded-full border border-border/60 bg-transparent px-3 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground transition-colors hover:border-border hover:text-foreground"
							>
								<svg
									className={`h-3 w-3 transition-transform ${sortOrder === "oldest" ? "rotate-180" : ""}`}
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<path d="M12 5v14" />
									<path d="m19 12-7 7-7-7" />
								</svg>
								{sortOrder}
							</button>
						</div>
					</div>
				)}
			</div>

			{/* ── List ── */}
			<div className="relative flex-1 overflow-y-auto overflow-x-hidden px-12 pb-10">
				{loading && remoteSessions.length === 0 ? (
					<div className="grid gap-px overflow-hidden rounded-lg border border-border bg-border">
						{[0, 1, 2, 3].map((i) => (
							<div key={i} className="h-[72px] animate-pulse bg-card" />
						))}
					</div>
				) : filteredAndSorted.length === 0 ? (
					<div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/70 bg-card/30 py-20 text-center">
						<div className="flex h-14 w-14 items-center justify-center rounded-full border border-border bg-background">
							<svg
								className="h-6 w-6 text-muted-foreground/50"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="1.5"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<rect x="3" y="4" width="18" height="16" rx="2" />
								<path d="M8 2v4" />
								<path d="M16 2v4" />
								<path d="M3 10h18" />
							</svg>
						</div>
						<h3 className="mt-5 text-[14px] font-medium text-foreground">
							{remoteSessions.length === 0 ? "No sessions yet" : "Nothing matches that filter"}
						</h3>
						<p className="mt-1.5 max-w-[320px] text-[12px] leading-relaxed text-muted-foreground">
							{remoteSessions.length === 0
								? "When someone invites you to a signing ceremony it will appear here."
								: "Try clearing the filter or refreshing the list."}
						</p>
					</div>
				) : (
					<div className="grid gap-px overflow-hidden rounded-lg border border-border bg-border">
						{filteredAndSorted.map((session) => {
							const isActive = session.status.toLowerCase() === "active";
							const isConnecting = connectingTo === session.id;
							return (
								<div
									key={session.id}
									className="group relative flex items-center gap-5 bg-card px-5 py-4 transition-colors hover:bg-muted/60"
								>
									{/* Type icon */}
									<div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-border bg-background text-muted-foreground">
										<svg
											className="h-4 w-4"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											strokeWidth="1.6"
											strokeLinecap="round"
											strokeLinejoin="round"
										>
											<path d="M12 2 4 6v6c0 5 3.5 9 8 10 4.5-1 8-5 8-10V6z" />
										</svg>
									</div>

									{/* Meta */}
									<div className="flex flex-1 flex-col gap-1 overflow-hidden">
										<div className="flex items-center gap-2">
											<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-accent">
												{session.session_type}
											</span>
											<span className="h-1 w-1 rounded-full bg-muted-foreground/30" />
											<span className="flex items-center gap-1.5">
												<span
													className={`h-1.5 w-1.5 rounded-full ${
														isActive
															? "bg-success shadow-[0_0_6px_1px] shadow-success/40"
															: "bg-muted-foreground/40"
													}`}
												/>
												<span className="font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
													{session.status}
												</span>
											</span>
										</div>
										<div className="flex items-center gap-3 font-mono text-[11px] tabular-nums text-foreground">
											<span className="truncate">{shortId(session.id)}</span>
											{session.created_at && (
												<>
													<span className="text-muted-foreground/40">·</span>
													<span className="text-muted-foreground">
														{formatRelative(session.created_at)}
													</span>
												</>
											)}
										</div>
									</div>

									{/* Connect button */}
									<button
										type="button"
										onClick={() => handleConnect(session.id)}
										disabled={isConnecting}
										className="flex h-9 shrink-0 items-center gap-2 rounded-md border border-primary/30 bg-primary/[0.06] px-4 font-mono text-[10px] uppercase tracking-[0.14em] text-foreground transition-all hover:border-primary/60 hover:bg-primary/[0.12] disabled:cursor-not-allowed disabled:opacity-60"
									>
										{isConnecting ? (
											<>
												<svg
													className="h-3 w-3 animate-spin"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													strokeWidth="2"
													strokeLinecap="round"
												>
													<path d="M21 12a9 9 0 1 1-6.219-8.56" />
												</svg>
												joining
											</>
										) : (
											<>
												join
												<svg
													className="h-3 w-3 transition-transform group-hover:translate-x-0.5"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													strokeWidth="2"
													strokeLinecap="round"
													strokeLinejoin="round"
												>
													<path d="M5 12h14" />
													<path d="m12 5 7 7-7 7" />
												</svg>
											</>
										)}
									</button>
								</div>
							);
						})}
					</div>
				)}
			</div>
		</div>
	);
}
