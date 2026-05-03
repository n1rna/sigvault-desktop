// Event types for backend-frontend communication

export type AppMode = "cloud" | "local";

export type WindowApplicationRoute =
	| "Loading"
	| "ModeChooser"
	| "SelectEnv"
	| "Login"
	| "MainPage"
	| "MachineRegistration"
	| "RemoteSessions"
	| "SessionDetails"
	| "LocalWallets";

export interface EnvironmentConfig {
	id: string;
	name: string;
	network: string;
	apiBaseUrl: string;
	comingSoon?: boolean;
}

export interface EnvironmentsResponse {
	environments: EnvironmentConfig[];
	selected_id: string | null;
}

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
	app_mode?: AppMode;
}

export interface CommandEvent {
	command: string;
	payload: Record<string, unknown>;
}

export interface SessionWorkflowPayload {
	session_type: string;
	step: number;
	requirements?: Record<string, unknown>;
	data?: Record<string, unknown>;
	finished?: boolean;
	message?: string;
	success?: boolean;
}

export interface NotificationEvent {
	level: NotificationLevel;
	title: string;
	message: string;
	duration_ms?: number;
}

export type LocalWalletSyncPhase =
	| "connecting"
	| "fetching_history"
	| "persisting"
	| "complete";

export interface LocalWalletSyncProgress {
	wallet_id: string;
	phase: LocalWalletSyncPhase;
	/** 0..=100. Coarse — the backend only emits boundaries between
	 * phases, since bdk_electrum's full_scan does not expose intra-scan
	 * progress callbacks. */
	percent: number;
	message: string;
}

export interface LocalWalletSyncSummary {
	wallet_id: string;
	tip_height: number;
	txs_synced: number;
	balance_sat: number;
}

export type AppEvent =
	| { type: "state_update"; data: StateUpdateEvent }
	| { type: "command"; data: CommandEvent }
	| { type: "session"; data: SessionWorkflowPayload }
	| { type: "notification"; data: NotificationEvent };

export interface AppState {
	authenticated: boolean;
	route: WindowApplicationRoute;
	/** Top-level app mode chosen at launch. `null` means the user has not
	 * picked yet (or has explicitly cleared the choice via Settings) and
	 * the Mode Chooser screen should be shown. */
	appMode: AppMode | null;
	listenerReady: boolean;
	notification: NotificationEvent | null;
	activityLog: NotificationEvent[];
	activeSession: {
		isConnected: boolean;
		sessionId?: string;
		sessionState?: {
			sessionType?: string;
			step?: number;
			requirements?: Record<string, unknown>;
			data?: Record<string, unknown>;
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
	created_at?: string;
}

export interface CommandResult<T = unknown> {
	success: boolean;
	message: string;
	data?: T;
	error?: {
		code: string;
		message: string;
	};
}
