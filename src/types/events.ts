// Event types for backend-frontend communication

export type WindowApplicationRoute =
	| "Loading"
	| "Login"
	| "MainPage"
	| "MachineRegistration"
	| "RemoteSessions"
	| "SessionDetails";

export type SessionMessageType =
	| "AuthorizationSuccess"
	| "WorkflowSession"
	| "SessionSuccess"
	| "SessionFailure"
	| "DeviceCreation"
	| "DeviceDeletion"
	| "SignTransaction";

export type NotificationLevel = "info" | "success" | "warning" | "error";

export interface StateUpdateEvent {
	route?: WindowApplicationRoute;
	active_session?: {
		is_connected: boolean;
		session_id: string;
		session_type: string;
	};
	remote_sessions?: RemoteSession[];
}

export interface CommandEvent {
	command: string;
	payload: any;
}

export interface SessionEvent {
	message_type: SessionMessageType;
	payload: any;
}

export interface NotificationEvent {
	level: NotificationLevel;
	title: string;
	message: string;
	duration_ms?: number;
}

export type AppEvent =
	| { type: "state_update"; data: StateUpdateEvent }
	| { type: "command"; data: CommandEvent }
	| { type: "session"; data: SessionEvent }
	| { type: "notification"; data: NotificationEvent };

export interface AppState {
	route: WindowApplicationRoute;
	activeSession: {
		isConnected: boolean;
		sessionId?: string;
		sessionType?: string;
		sessionState?: {
			step?: number;
			requirements?: any;
			finished?: boolean;
			message?: string;
			error?: string;
		};
	};
	remoteSessions: RemoteSession[];
}

export interface RemoteSession {
	id: string;
	status: string;
	session_type: string;
}

export interface CommandResult {
	success: boolean;
	message: string;
	error?: {
		code: string;
		message: string;
	};
}
