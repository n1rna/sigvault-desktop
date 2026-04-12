import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { useAppState } from "../contexts/AppStateContext";

const appWindow = getCurrentWindow();

function useDrag() {
	return useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			appWindow.startDragging();
		}
	}, []);
}

const navItems = [
	{ label: "Home", route: "MainPage" },
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
			className="flex h-10 shrink-0 select-none items-center border-b border-border bg-card"
		>
			<span className="px-4 font-mono text-xs font-bold tracking-wider text-primary">
				SIGVAULT
				{appVersion && (
					<span className="ml-1.5 font-normal text-muted-foreground/50">v{appVersion}</span>
				)}
			</span>

			<div className="flex items-center" onMouseDown={(e) => e.stopPropagation()}>
				{navItems.map((item) => {
					const isActive = route === item.route;
					return (
						<button
							key={item.route}
							type="button"
							onClick={() => navigate(item.route)}
							className={`relative bg-transparent px-3 py-2.5 text-xs font-medium transition-colors ${
								isActive
									? "text-foreground"
									: "text-muted-foreground hover:text-foreground"
							}`}
						>
							{item.label}
							{isActive && (
								<span className="absolute bottom-0 left-3 right-3 h-px bg-primary" />
							)}
						</button>
					);
				})}
			</div>

			<div className="flex-1" />

			<button
				type="button"
				onClick={handleSignOut}
				disabled={signingOut}
				onMouseDown={(e) => e.stopPropagation()}
				className="bg-transparent px-3 py-1 text-xs text-muted-foreground transition-colors hover:text-destructive disabled:opacity-50"
			>
				{signingOut ? "Signing out…" : "Sign Out"}
			</button>

			<div className="mx-1 h-4 w-px bg-border" />

			<button
				type="button"
				onMouseDown={(e) => e.stopPropagation()}
				onClick={() => appWindow.minimize()}
				className="inline-flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
			>
				<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
					<path d="M5 12h14" />
				</svg>
			</button>
			<button
				type="button"
				onMouseDown={(e) => e.stopPropagation()}
				onClick={() => appWindow.close()}
				className="inline-flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
			>
				<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
					<path d="M18 6 6 18" />
					<path d="m6 6 12 12" />
				</svg>
			</button>
		</nav>
	);
}
