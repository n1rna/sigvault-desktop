import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppState } from "../contexts/AppStateContext";

const navItems = [
	{ label: "Home", route: "MainPage" },
	{ label: "Sessions", route: "RemoteSessions" },
];

export default function Navbar() {
	const { route } = useAppState();
	const [signingOut, setSigningOut] = useState(false);

	const navigate = async (target: string) => {
		try {
			await invoke("cmd_navigate", { route: target });
		} catch (error) {
			console.error("Failed to navigate:", error);
		}
	};

	const handleSignOut = async () => {
		setSigningOut(true);
		try {
			await invoke("cmd_logout");
		} catch (error) {
			console.error("Failed to sign out:", error);
			setSigningOut(false);
		}
	};

	return (
		<nav className="flex h-10 shrink-0 items-center gap-1 border-b border-border bg-card px-4">
			<span className="mr-4 text-sm font-semibold text-foreground">
				SigVault
			</span>
			{navItems.map((item) => {
				const isActive = route === item.route;
				return (
					<button
						key={item.route}
						type="button"
						onClick={() => navigate(item.route)}
						className={`bg-transparent px-3 py-1.5 text-sm ${
							isActive
								? "text-primary"
								: "text-muted-foreground hover:text-foreground"
						}`}
					>
						{item.label}
					</button>
				);
			})}
			<div className="ml-auto">
				<button
					type="button"
					onClick={handleSignOut}
					disabled={signingOut}
					className="bg-transparent px-3 py-1.5 text-sm text-muted-foreground hover:text-destructive disabled:opacity-50"
				>
					{signingOut ? "Signing out…" : "Sign Out"}
				</button>
			</div>
		</nav>
	);
}
