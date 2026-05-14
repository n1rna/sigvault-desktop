// Hook for listening to backend events and managing app state

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useReducer, useState } from "react";
import type {
	AppEvent,
	AppState,
	CommandEvent,
	NotificationEvent,
	RemoteSession,
	SessionWorkflowPayload,
	StateUpdateEvent,
} from "../types/events";

const initialState: AppState = {
	authenticated: false,
	route: "Loading",
	appMode: null,
	listenerReady: false,
	notification: null,
	activityLog: [],
	activeSession: {
		isConnected: false,
		sessionId: undefined,
		sessionState: {},
	},
	remoteSessions: [],
};

type AppAction =
	| { type: "UPDATE_STATE"; payload: Partial<AppState> }
	| { type: "SET_REMOTE_SESSIONS"; payload: RemoteSession[] }
	| { type: "UPDATE_SESSION_STATE"; payload: Record<string, unknown> }
	| { type: "PUSH_ACTIVITY"; payload: NotificationEvent }
	| { type: "CLEAR_ACTIVITY" };

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
		case "PUSH_ACTIVITY":
			return { ...state, activityLog: [...state.activityLog, action.payload] };
		case "CLEAR_ACTIVITY":
			return { ...state, activityLog: [] };
		default:
			return state;
	}
}

export function useAppEvents() {
	const [state, dispatch] = useReducer(appStateReducer, initialState);
	const [listenerReady, setListenerReady] = useState(false);

	const handleStateUpdate = useCallback((data: StateUpdateEvent) => {
		const updates: Partial<AppState> = {};

		if (data.authenticated != null) {
			updates.authenticated = data.authenticated;
		}
		if (data.route != null) {
			updates.route = data.route;
			// The ModeChooser route is the canonical "no mode selected"
			// signal — clear appMode whenever we land there so the chooser
			// screen never sees a stale Cloud/Local label.
			if (data.route === "ModeChooser") {
				updates.appMode = null;
			}
		}
		if (data.app_mode != null) {
			updates.appMode = data.app_mode;
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
		switch (data.command) {
			case "update_remote_sessions":
				if (data.payload?.sessions) {
					dispatch({
						type: "SET_REMOTE_SESSIONS",
						payload: data.payload.sessions as unknown as RemoteSession[],
					});
				}
				break;
		}
	}, []);

	const handleSession = useCallback((data: SessionWorkflowPayload) => {
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
		dispatch({ type: "UPDATE_STATE", payload: { notification: data } });
		dispatch({ type: "PUSH_ACTIVITY", payload: data });
	}, []);

	useEffect(() => {
		const unlistenPromise = listen<AppEvent>("app_event", (event) => {
			const appEvent = event.payload;

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
			}
		});

		unlistenPromise.then(() => {
			setListenerReady(true);
		});

		return () => {
			unlistenPromise.then((unlisten) => unlisten());
		};
	}, [handleStateUpdate, handleCommand, handleSession, handleNotification]);

	const clearActivityLog = useCallback(() => {
		dispatch({ type: "CLEAR_ACTIVITY" });
	}, []);

	return { ...state, listenerReady, clearActivityLog };
}
