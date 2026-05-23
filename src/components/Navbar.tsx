import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useState } from "react";
import { useAppState } from "../contexts/AppStateContext";
import { SigVaultLogo } from "./SigVaultLogo";
import SwitchModeButton from "./SwitchModeButton";

const appWindow = getCurrentWindow();

function useDrag() {
	return useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			appWindow.startDragging();
		}
	}, []);
}

const navItems: Array<{ label: string; route: string }> = [
	{ label: "Overview", route: "MainPage" },
	{ label: "Sessions", route: "RemoteSessions" },
];

export default function Navbar() {
	const { route } = useAppState();
	const [signingOut, setSigningOut] = useState(false);
	const [appVersion, setAppVersion] = useState("");
	const onDrag = useDrag();

	useEffect(() => {
		getVersion().then(setAppVersion);
	}, []);

	const navigate = async (target: string) => {
		try {
			await invoke("cmd_navigate", { route: target });
		} catch {
			// Navigation handled by backend
		}
	};

	const handleSignOut = async () => {
		setSigningOut(true);
		try {
			await invoke("cmd_logout");
		} catch {
			setSigningOut(false);
		}
	};

	return (
		<nav
			onMouseDown={onDrag}
			className="relative flex h-14 shrink-0 select-none items-stretch border-b border-border bg-card/80 backdrop-blur-sm"
		>
			{/* ── Brand ── */}
			<div className="flex items-center gap-3 pl-5 pr-6">
				<SigVaultLogo className="h-6 w-6 text-foreground" />
				<div className="flex flex-col leading-none">
					<span className="font-mono text-[11px] font-bold tracking-[0.22em] text-foreground">
						SIGVAULT
					</span>
					{appVersion && (
						<span className="mt-1 font-mono text-[9px] uppercase tracking-[0.18em] text-muted-foreground/60">
							v{appVersion}
						</span>
					)}
				</div>
			</div>

			{/* ── Divider ── */}
			<div className="my-3 w-px bg-border/60" />

			{/* ── Nav items ── */}
			<div className="flex items-center gap-1 px-4" onMouseDown={(e) => e.stopPropagation()}>
				{navItems.map((item) => {
					const isActive = route === item.route;
					return (
						<button
							key={item.route}
							type="button"
							onClick={() => navigate(item.route)}
							className={`relative flex h-8 items-center rounded-md px-3.5 font-mono text-[11px] uppercase tracking-[0.14em] transition-colors ${
								isActive
									? "bg-primary/[0.08] text-foreground"
									: "text-muted-foreground hover:bg-muted/60 hover:text-foreground"
							}`}
						>
							{item.label}
							{isActive && (
								<span className="absolute -bottom-[1px] left-3 right-3 h-[2px] rounded-full bg-primary" />
							)}
						</button>
					);
				})}
			</div>

			<div className="flex-1" />

			{/* ── Status pill ── */}
			<div className="flex items-center gap-2 pr-5" onMouseDown={(e) => e.stopPropagation()}>
				<div className="flex h-7 items-center gap-2 rounded-full border border-border/80 bg-background/60 px-3">
					<span className="relative flex h-1.5 w-1.5">
						<span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-60" />
						<span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-success" />
					</span>
					<span className="font-mono text-[9px] uppercase tracking-[0.2em] text-muted-foreground">
						connected
					</span>
				</div>
				<SwitchModeButton />
			</div>

			{/* ── Sign out ── */}
			<button
				type="button"
				onClick={handleSignOut}
				disabled={signingOut}
				onMouseDown={(e) => e.stopPropagation()}
				className="flex items-center gap-1.5 border-l border-border/60 bg-transparent px-4 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors hover:text-destructive disabled:opacity-50"
			>
				<svg
					className="h-3 w-3"
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
				{signingOut ? "Exiting…" : "Sign out"}
			</button>

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

			{/* ── Accent hairline ── */}
			<div className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-gradient-to-r from-transparent via-primary/40 to-transparent" />
		</nav>
	);
}
