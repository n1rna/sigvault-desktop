import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
	const onDrag = useDrag();

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
			className="flex h-9 shrink-0 select-none items-center border-b border-border bg-card"
		>
			<span className="px-3 text-xs font-semibold text-foreground">
				SigVault
			</span>

			<div className="flex items-center gap-0.5" onMouseDown={(e) => e.stopPropagation()}>
				{navItems.map((item) => {
					const isActive = route === item.route;
					return (
						<button
							key={item.route}
							type="button"
							onClick={() => navigate(item.route)}
							className={`bg-transparent px-2.5 py-1 text-xs ${
								isActive
									? "text-primary"
									: "text-muted-foreground hover:text-foreground"
							}`}
						>
							{item.label}
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
				className="bg-transparent px-2.5 py-1 text-xs text-muted-foreground hover:text-destructive disabled:opacity-50"
			>
				{signingOut ? "Signing out..." : "Sign Out"}
			</button>

			<button
				type="button"
				onMouseDown={(e) => e.stopPropagation()}
				onClick={() => appWindow.minimize()}
				className="inline-flex h-full w-9 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground"
			>
				<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
					<path d="M5 12h14" />
				</svg>
			</button>
			<button
				type="button"
				onMouseDown={(e) => e.stopPropagation()}
				onClick={() => appWindow.close()}
				className="inline-flex h-full w-9 items-center justify-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"
			>
				<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
					<path d="M18 6 6 18" />
					<path d="m6 6 12 12" />
				</svg>
			</button>
		</nav>
	);
}
