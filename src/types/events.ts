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
	authenticated?: boolean;
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

export interface SessionWorkflowPayload {
	session_type: string;
	step: number;
	requirements?: any;
	finished?: boolean;
	message?: string;
	success?: boolean;
}

// export interface SessionEvent {
// 	message_type: SessionMessageType;
// 	payload: SessionWorkflowPayload;
// }

export interface NotificationEvent {
	level: NotificationLevel;
	title: string;
	message: string;
	duration_ms?: number;
}

export type AppEvent =
	| { type: "state_update"; data: StateUpdateEvent }
	| { type: "command"; data: CommandEvent }
	| { type: "session"; data: SessionWorkflowPayload }
	| { type: "notification"; data: NotificationEvent };

export interface AppState {
	authenticated: boolean;
	route: WindowApplicationRoute;
	activeSession: {
		isConnected: boolean;
		sessionId?: string;
		sessionState?: {
			sessionType?: string;
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

export interface CommandResult<T = any> {
	success: boolean;
	message: string;
	data?: T;
	error?: {
		code: string;
		message: string;
	};
}
