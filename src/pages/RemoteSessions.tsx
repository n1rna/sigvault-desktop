import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult, RemoteSession } from "../types/events";
import { useAppState } from "../contexts/AppStateContext";

type SortOrder = "newest" | "oldest";

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
			const sessions =
				await invoke<RemoteSession[]>("cmd_get_remote_sessions");
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
			const result = await invoke<CommandResult>(
				"cmd_start_session_websocket_connection",
				{ sessionId },
			);
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
				const diff =
					new Date(b.created_at).getTime() -
					new Date(a.created_at).getTime();
				if (diff !== 0)
					return sortOrder === "newest" ? diff : -diff;
			}
			// Fall back to original order (backend returns oldest first)
			return sortOrder === "newest"
				? b._index - a._index
				: a._index - b._index;
		});

		return sorted;
	}, [remoteSessions, typeFilter, sortOrder]);

	return (
		<div className="relative flex h-full w-full flex-col overflow-hidden">
			{error && (
				<div className="mx-8 mt-8 border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
					{error}
				</div>
			)}
			<div className="shrink-0 px-8 pt-8 pb-4">
				<div className="flex items-center justify-between">
					<h1 className="text-[1.75rem] font-semibold text-foreground">
						Remote Sessions
					</h1>
					<button
						type="button"
						onClick={fetchSessions}
						disabled={loading}
						className="bg-transparent p-2 text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
						title="Refresh sessions"
					>
						<svg
							width="18"
							height="18"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							className={loading ? "animate-spin" : ""}
						>
							<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
							<path d="M3 3v5h5" />
							<path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
							<path d="M16 16h5v5" />
						</svg>
					</button>
				</div>

				{remoteSessions.length > 0 && (
					<div className="mt-4 flex items-center gap-4">
						<div className="flex items-center gap-1.5">
							{["all", ...sessionTypes].map((type) => (
								<button
									key={type}
									type="button"
									onClick={() => setTypeFilter(type)}
									className={`px-2.5 py-1 text-xs ${
										typeFilter === type
											? "bg-primary text-primary-foreground"
											: "bg-secondary text-muted-foreground hover:text-foreground"
									}`}
								>
									{type === "all" ? "All" : type}
								</button>
							))}
						</div>

						<button
							type="button"
							onClick={() =>
								setSortOrder((s) =>
									s === "newest" ? "oldest" : "newest",
								)
							}
							className="flex items-center gap-1.5 bg-transparent px-2 py-1 text-xs text-muted-foreground hover:text-foreground"
						>
							<svg
								width="12"
								height="12"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2"
								className={
									sortOrder === "oldest"
										? "rotate-180"
										: ""
								}
							>
								<path d="m3 8 4-4 4 4" />
								<path d="M7 4v16" />
								<path d="M17 20V4" />
								<path d="m21 16-4 4-4-4" />
							</svg>
							{sortOrder === "newest"
								? "Newest"
								: "Oldest"}
						</button>

						{typeFilter !== "all" && (
							<span className="text-xs text-muted-foreground">
								{filteredAndSorted.length} of{" "}
								{remoteSessions.length}
							</span>
						)}
					</div>
				)}

				<div className="absolute right-0 left-0 h-6 bg-gradient-to-b from-background to-transparent" />
			</div>

			<div className="flex-1 overflow-y-auto overflow-x-hidden px-8 pb-8">
				<div className="grid gap-3">
					{loading && remoteSessions.length === 0 ? (
						<>
							<div className="h-16 animate-pulse bg-muted" />
							<div className="h-16 animate-pulse bg-muted" />
							<div className="h-16 animate-pulse bg-muted" />
						</>
					) : filteredAndSorted.length === 0 ? (
						<p className="py-12 text-center text-muted-foreground">
							{remoteSessions.length === 0
								? "No sessions available"
								: "No sessions match the filter"}
						</p>
					) : (
						filteredAndSorted.map((session) => (
							<div
								key={session.id}
								className="flex items-center justify-between border border-border bg-card px-4 py-3 hover:border-primary"
							>
								<div className="flex items-center gap-3 overflow-hidden">
									<span className="shrink-0 border border-border bg-secondary px-2 py-0.5 text-xs text-secondary-foreground">
										{session.session_type}
									</span>
									<span className="truncate font-mono text-xs text-muted-foreground">
										{session.id}
									</span>
									<span
										className={`shrink-0 px-2 py-0.5 text-xs font-medium ${
											session.status.toLowerCase() ===
											"active"
												? "bg-green-500/10 text-green-500"
												: "bg-primary/10 text-primary"
										}`}
									>
										{session.status}
									</span>
									{session.created_at && (
										<span className="shrink-0 text-xs text-muted-foreground">
											{new Date(
												session.created_at,
											).toLocaleDateString()}
										</span>
									)}
								</div>
								<button
									type="button"
									onClick={() => handleConnect(session.id)}
									disabled={connectingTo === session.id}
									className="shrink-0 bg-transparent p-2 text-muted-foreground hover:text-primary disabled:cursor-not-allowed disabled:opacity-50"
									title="Connect to session"
								>
									{connectingTo === session.id ? (
										<svg
											width="16"
											height="16"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											strokeWidth="2"
											className="animate-spin"
										>
											<path d="M21 12a9 9 0 1 1-6.219-8.56" />
										</svg>
									) : (
										<svg
											width="16"
											height="16"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											strokeWidth="2"
										>
											<path d="M5 12h14" />
											<path d="m12 5 7 7-7 7" />
										</svg>
									)}
								</button>
							</div>
						))
					)}
				</div>
			</div>
		</div>
	);
}
