// Hook for listening to backend events and managing app state

import { useEffect, useReducer, useCallback, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
	AppEvent,
	AppState,
	StateUpdateEvent,
	CommandEvent,
	NotificationEvent,
	SessionWorkflowPayload,
} from "../types/events";

const initialState: AppState = {
	authenticated: false,
	route: "Loading",
	activeSession: {
		isConnected: false,
		sessionId: undefined,
		sessionState: {},
	},
	remoteSessions: [],
};

type AppAction =
	| { type: "UPDATE_STATE"; payload: Partial<AppState> }
	| { type: "SET_REMOTE_SESSIONS"; payload: any[] }
	| { type: "UPDATE_SESSION_STATE"; payload: any };

function appStateReducer(state: AppState, action: AppAction): AppState {
	switch (action.type) {
		case "UPDATE_STATE":
			return { ...state, ...action.payload };
		case "SET_REMOTE_SESSIONS":
			return { ...state, remoteSessions: action.payload };
		case "UPDATE_SESSION_STATE":
			return {
				...state,
				activeSession: {
					...state.activeSession,
					sessionState: {
						...state.activeSession.sessionState,
						...action.payload,
					},
				},
			};
		default:
			return state;
	}
}

export function useAppEvents() {
	const [state, dispatch] = useReducer(appStateReducer, initialState);
	const [listenerReady, setListenerReady] = useState(false);

	const handleStateUpdate = useCallback((data: StateUpdateEvent) => {
		console.log("State update:", data);

		const updates: Partial<AppState> = {};

		if (data.authenticated != null) {
			updates.authenticated = data.authenticated;
		}
		if (data.route != null) {
			updates.route = data.route;
		}
		if (data.active_session != null) {
			const activeSession: Partial<AppState["activeSession"]> = {};

			if (data.active_session.is_connected != null) {
				activeSession.isConnected = data.active_session.is_connected;
			}
			if (data.active_session.session_id != null) {
				activeSession.sessionId = data.active_session.session_id;
			}

			if (Object.keys(activeSession).length > 0) {
				updates.activeSession = activeSession as AppState["activeSession"];
			}
		}
		if (data.remote_sessions != null) {
			updates.remoteSessions = data.remote_sessions;
		}

		if (Object.keys(updates).length > 0) {
			dispatch({ type: "UPDATE_STATE", payload: updates });
		}
	}, []);

	const handleCommand = useCallback((data: CommandEvent) => {
		console.log("Command received:", data.command, data.payload);

		switch (data.command) {
			case "update_remote_sessions":
				if (data.payload?.sessions) {
					dispatch({
						type: "SET_REMOTE_SESSIONS",
						payload: data.payload.sessions,
					});
				}
				break;
			case "register_machine":
				console.log("Register machine requested:", data.payload);
				break;
			default:
				console.log("Unhandled command:", data.command);
		}
	}, []);

	const handleSession = useCallback((data: SessionWorkflowPayload) => {
		console.log("Session event:", data);

		if (data?.success) {
			dispatch({
				type: "UPDATE_SESSION_STATE",
				payload: {
					sessionType: data.session_type,
					step: data.step,
					requirements: data.requirements,
					data: data.data,
					finished: data.finished,
					message: data.message,
				},
			});
		} else {
			dispatch({
				type: "UPDATE_SESSION_STATE",
				payload: {
					error: data?.message || "Unknown error",
				},
			});
		}
	}, []);

	const handleNotification = useCallback((data: NotificationEvent) => {
		console.log(`[${data.level.toUpperCase()}] ${data.title}:`, data.message);

		// TODO: Show actual notification UI (toast)
		// For now, we'll just log it
		if (data.level === "error") {
			console.error(`${data.title}: ${data.message}`);
		}
	}, []);

	useEffect(() => {
		console.log("Setting up event listener...");

		const unlistenPromise = listen<AppEvent>("app_event", (event) => {
			const appEvent = event.payload;
			console.log("Received app_event:", appEvent);

			switch (appEvent.type) {
				case "state_update":
					handleStateUpdate(appEvent.data);
					break;
				case "command":
					handleCommand(appEvent.data);
					break;
				case "session":
					handleSession(appEvent.data);
					break;
				case "notification":
					handleNotification(appEvent.data);
					break;
				default:
					console.warn("Unknown event type:", appEvent);
			}
		});

		unlistenPromise.then(() => {
			console.log("Event listener ready");
			setListenerReady(true);
		});

		return () => {
			console.log("Cleaning up event listener...");
			unlistenPromise.then((unlisten) => unlisten());
		};
	}, [handleStateUpdate, handleCommand, handleSession, handleNotification]);

	return { ...state, listenerReady };
}
