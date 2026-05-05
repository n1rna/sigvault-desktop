import React, { createContext, useContext } from "react";
import { useAppEvents } from "../hooks/useAppEvents";
import type { AppState } from "../types/events";

type AppStateContextType = AppState & { clearActivityLog: () => void };

export const AppStateContext = createContext<AppStateContextType | null>(null);

export function AppStateProvider({ children }: { children: React.ReactNode }) {
	const state = useAppEvents();

	return (
		<AppStateContext.Provider value={state}>
			{children}
		</AppStateContext.Provider>
	);
}

export function useAppState() {
	const context = useContext(AppStateContext);
	if (!context) {
		throw new Error("useAppState must be used within AppStateProvider");
	}
	return context;
}
