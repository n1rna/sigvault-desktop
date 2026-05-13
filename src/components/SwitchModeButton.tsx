// Shared "Switch mode" affordance.
//
// Calls `cmd_clear_app_mode`, which forgets the persisted mode and routes
// the frontend back to the ModeChooser screen. Auto-detects the current
// mode from AppStateContext so the label reflects the *target* mode, not
// the source. Renders nothing when no mode is set (i.e. the user is
// already on the mode chooser).

import { invoke } from "@tauri-apps/api/core";
import { useContext } from "react";
import { AppStateContext } from "../contexts/AppStateContext";

interface Props {
	className?: string;
}

export default function SwitchModeButton({ className = "" }: Props) {
	// Read the context directly (rather than via useAppState) so the
	// component can render null in tests / storybook contexts that don't
	// wrap their tree in AppStateProvider, instead of throwing.
	const ctx = useContext(AppStateContext);
	const appMode = ctx?.appMode ?? null;
	if (!appMode) return null;
	const target = appMode === "cloud" ? "Local Wallet" : "SigVault Cloud";

	const switchMode = () => {
		void invoke("cmd_clear_app_mode");
	};

	return (
		<button
			type="button"
			onClick={switchMode}
			className={`inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card/60 px-3 font-mono text-[9px] uppercase tracking-[0.20em] text-muted-foreground backdrop-blur-sm transition-colors hover:border-primary/40 hover:text-foreground ${className}`}
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
				<path d="M16 3h5v5" />
				<path d="M8 21H3v-5" />
				<path d="M21 3l-7 7" />
				<path d="M3 21l7-7" />
			</svg>
			Switch to {target}
		</button>
	);
}
