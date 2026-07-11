// Global top bar — rendered on every page.
//
// Owns the app's window chrome (drag region + minimize/close) and surfaces
// cross-cutting state: login status, a Local ⇄ Cloud context switcher, and
// (only when signed in) cloud navigation + sign out. It adapts to state:
// pre-context screens (Welcome / Loading) simply show fewer controls.

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import type { AppMode } from "../types/events";
import { SigVaultLogo } from "./SigVaultLogo";

const appWindow = getCurrentWindow();

interface UserInfo {
	email?: string;
	name?: string;
	username?: string;
}

function useDrag() {
	return useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			appWindow.startDragging();
		}
	}, []);
}

export default function TopBar() {
	const { appMode, authenticated } = useAppState();
	const [signingOut, setSigningOut] = useState(false);
	const [appVersion, setAppVersion] = useState("");
	const [user, setUser] = useState<UserInfo | null>(null);
	const onDrag = useDrag();

	useEffect(() => {
		getVersion().then(setAppVersion);
	}, []);

	// Pull the profile for the status pill whenever auth flips on.
	useEffect(() => {
		if (!authenticated) {
			setUser(null);
			return;
		}
		invoke<UserInfo>("cmd_get_current_user")
			.then(setUser)
			.catch(() => setUser(null));
	}, [authenticated]);

	const handleSignOut = async () => {
		setSigningOut(true);
		try {
			await invoke("cmd_logout");
		} catch {
			setSigningOut(false);
		}
	};

	// Kick off the cloud sign-in flow (switches to cloud mode, which routes
	// to SelectEnv / Login). Used by the account button when signed out.
	const signIn = () => {
		void invoke("cmd_set_app_mode", { mode: "cloud" });
	};

	return (
		<nav
			onMouseDown={onDrag}
			className="relative z-50 flex h-14 shrink-0 select-none items-stretch border-b border-border bg-card/80 backdrop-blur-sm"
		>
			{/* ── Brand ── */}
			<div className="flex items-center gap-2.5 pl-4 pr-5">
				<SigVaultLogo className="h-5 w-5 text-foreground" />
				<div className="flex items-baseline gap-2 leading-none">
					<span className="text-[14px] font-semibold tracking-tight text-foreground">SigVault</span>
					{appVersion && (
						<span className="text-[11px] tabular-nums text-muted-foreground/60">v{appVersion}</span>
					)}
				</div>
			</div>

			<div className="flex-1" />

			{/* ── Context switcher ── */}
			<div className="flex items-center px-3" onMouseDown={(e) => e.stopPropagation()}>
				<ContextSwitcher appMode={appMode} />
			</div>

			{/* ── Account (full-height divider, matching window controls) ── */}
			<div
				className="flex items-center border-l border-border/60 px-3"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<AccountMenu
					authenticated={authenticated}
					email={user?.email || user?.name || user?.username || null}
					signingOut={signingOut}
					onSignOut={handleSignOut}
					onSignIn={signIn}
				/>
			</div>

			{/* ── Window controls ── */}
			<div
				className="flex items-stretch border-l border-border/60"
				onMouseDown={(e) => e.stopPropagation()}
			>
				<button
					type="button"
					onClick={() => appWindow.minimize()}
					className="inline-flex w-11 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					aria-label="Minimize"
				>
					<svg
						className="h-3.5 w-3.5"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
					>
						<path d="M5 12h14" />
					</svg>
				</button>
				<button
					type="button"
					onClick={() => appWindow.close()}
					className="inline-flex w-11 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
					aria-label="Close"
				>
					<svg
						className="h-3.5 w-3.5"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
					>
						<path d="M18 6 6 18" />
						<path d="m6 6 12 12" />
					</svg>
				</button>
			</div>
		</nav>
	);
}

// Consolidated account control. Signed out → a single "sign in" icon
// button. Signed in → an avatar circle that opens a small menu showing the
// email and a Sign out action.
function AccountMenu({
	authenticated,
	email,
	signingOut,
	onSignOut,
	onSignIn,
}: {
	authenticated: boolean;
	email: string | null;
	signingOut: boolean;
	onSignOut: () => void;
	onSignIn: () => void;
}) {
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		if (!open) return;
		const onDown = (e: MouseEvent) => {
			if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") setOpen(false);
		};
		document.addEventListener("mousedown", onDown);
		document.addEventListener("keydown", onKey);
		return () => {
			document.removeEventListener("mousedown", onDown);
			document.removeEventListener("keydown", onKey);
		};
	}, [open]);

	if (!authenticated) {
		return (
			<button
				type="button"
				onClick={onSignIn}
				title="Sign in to SigVault Cloud"
				aria-label="Sign in"
				className="flex h-8 w-8 items-center justify-center rounded-full border border-border bg-card/60 text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
			>
				<svg
					className="h-4 w-4"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
					<circle cx="12" cy="7" r="4" />
				</svg>
			</button>
		);
	}

	const label = email || "Signed in";
	const initial = label.trim().charAt(0).toUpperCase() || "?";

	return (
		<div ref={ref} className="relative">
			<button
				type="button"
				onClick={() => setOpen((v) => !v)}
				title={label}
				aria-label="Account"
				className="flex h-8 w-8 items-center justify-center rounded-full border border-primary/30 bg-primary/[0.12] text-[12px] font-semibold text-primary transition-colors hover:border-primary/60"
			>
				{initial}
			</button>

			{open && (
				<div className="absolute right-0 top-[calc(100%+6px)] z-50 w-64 overflow-hidden rounded-lg border border-border bg-card shadow-xl">
					<div className="flex items-center gap-3 border-b border-border/60 px-3 py-3">
						<span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-primary/30 bg-primary/[0.12] text-[12px] font-semibold text-primary">
							{initial}
						</span>
						<span className="min-w-0 flex-1">
							<span className="block text-[11px] text-muted-foreground/70">Signed in as</span>
							<span className="mt-0.5 block truncate text-[12.5px] font-medium text-foreground">
								{label}
							</span>
						</span>
					</div>
					<button
						type="button"
						onClick={onSignOut}
						disabled={signingOut}
						className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left text-[13px] font-medium text-muted-foreground transition-colors hover:bg-muted/60 hover:text-destructive disabled:opacity-50"
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
							<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
							<polyline points="16 17 21 12 16 7" />
							<line x1="21" y1="12" x2="9" y2="12" />
						</svg>
						{signingOut ? "Signing out…" : "Sign out"}
					</button>
				</div>
			)}
		</div>
	);
}

const MODE_META: Record<AppMode, { title: string; hint: string }> = {
	local: { title: "Local Wallets", hint: "On-device · no account" },
	cloud: { title: "SigVault Cloud", hint: "Team multisig · sessions" },
};

// Local ⇄ Cloud context switcher. A small self-contained popover (no
// external dep). Selecting a mode calls `cmd_set_app_mode`, which persists
// the choice and routes onward.
function ContextSwitcher({ appMode }: { appMode: AppMode | null }) {
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);

	useEffect(() => {
		if (!open) return;
		const onDown = (e: MouseEvent) => {
			if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") setOpen(false);
		};
		document.addEventListener("mousedown", onDown);
		document.addEventListener("keydown", onKey);
		return () => {
			document.removeEventListener("mousedown", onDown);
			document.removeEventListener("keydown", onKey);
		};
	}, [open]);

	const select = (mode: AppMode) => {
		setOpen(false);
		if (mode === appMode) return;
		void invoke("cmd_set_app_mode", { mode });
	};

	const current = appMode ? MODE_META[appMode].title : "Choose context";

	return (
		<div ref={ref} className="relative">
			<button
				type="button"
				onClick={() => setOpen((v) => !v)}
				className="inline-flex h-8 w-44 items-center gap-2 rounded-md border border-border bg-card/60 px-3 text-[12px] font-medium text-muted-foreground backdrop-blur-sm transition-colors hover:border-primary/40 hover:text-foreground"
			>
				<span
					className={`h-1.5 w-1.5 shrink-0 rounded-full ${
						appMode === "cloud"
							? "bg-primary"
							: appMode === "local"
								? "bg-accent"
								: "bg-muted-foreground/50"
					}`}
				/>
				<span className="flex-1 truncate text-left">{current}</span>
				<svg
					className={`h-3 w-3 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
					strokeLinejoin="round"
				>
					<path d="m6 9 6 6 6-6" />
				</svg>
			</button>

			{open && (
				<div className="absolute right-0 top-[calc(100%+6px)] z-50 w-64 overflow-hidden rounded-lg border border-border bg-card shadow-xl">
					<div className="border-b border-border/60 px-3 py-2 text-[11px] font-medium text-muted-foreground/70">
						Switch context
					</div>
					{(Object.keys(MODE_META) as AppMode[]).map((mode) => {
						const meta = MODE_META[mode];
						const active = mode === appMode;
						return (
							<button
								key={mode}
								type="button"
								onClick={() => select(mode)}
								className={`flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-muted/60 ${
									active ? "bg-primary/[0.06]" : ""
								}`}
							>
								<span
									className={`mt-0.5 h-1.5 w-1.5 shrink-0 rounded-full ${
										mode === "cloud" ? "bg-primary" : "bg-accent"
									}`}
								/>
								<span className="flex-1 min-w-0">
									<span className="block text-[12.5px] font-medium text-foreground">
										{meta.title}
									</span>
									<span className="mt-0.5 block text-[11px] text-muted-foreground/70">
										{meta.hint}
									</span>
								</span>
								{active && (
									<svg
										className="h-3.5 w-3.5 shrink-0 text-primary"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										strokeWidth="2.5"
										strokeLinecap="round"
										strokeLinejoin="round"
									>
										<polyline points="20 6 9 17 4 12" />
									</svg>
								)}
							</button>
						);
					})}
				</div>
			)}
		</div>
	);
}
