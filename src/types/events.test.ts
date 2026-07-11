import { describe, expect, it } from "vitest";
import type {
	AppState,
	CommandEvent,
	CommandResult,
	NotificationEvent,
	RemoteSession,
	StateUpdateEvent,
	WindowApplicationRoute,
} from "./events";

describe("Event types", () => {
	it("AppState has correct default shape", () => {
		const state: AppState = {
			authenticated: false,
			route: "Loading",
			appMode: null,
			listenerReady: false,
			notification: null,
			activityLog: [],
			activeSession: { isConnected: false },
			remoteSessions: [],
		};

		expect(state.authenticated).toBe(false);
		expect(state.route).toBe("Loading");
		expect(state.appMode).toBeNull();
		expect(state.remoteSessions).toHaveLength(0);
	});

	it("StateUpdateEvent accepts partial updates", () => {
		const event: StateUpdateEvent = {
			authenticated: true,
			route: "MainPage",
		};

		expect(event.authenticated).toBe(true);
		expect(event.route).toBe("MainPage");
		expect(event.active_session).toBeUndefined();
	});

	it("StateUpdateEvent with active session", () => {
		const event: StateUpdateEvent = {
			active_session: {
				is_connected: true,
				session_id: "abc-123",
				session_type: "DEVICE_REGISTRATION",
			},
		};

		expect(event.active_session?.is_connected).toBe(true);
		expect(event.active_session?.session_id).toBe("abc-123");
	});

	it("CommandEvent has command and payload", () => {
		const event: CommandEvent = {
			command: "update_remote_sessions",
			payload: { sessions: [] },
		};

		expect(event.command).toBe("update_remote_sessions");
		expect(event.payload).toEqual({ sessions: [] });
	});

	it("NotificationEvent has required fields", () => {
		const event: NotificationEvent = {
			level: "success",
			title: "Connected",
			message: "Session connected successfully",
		};

		expect(event.level).toBe("success");
		expect(event.duration_ms).toBeUndefined();
	});

	it("RemoteSession has required fields", () => {
		const session: RemoteSession = {
			id: "session-1",
			status: "active",
			session_type: "TRANSACTION_SIGNING",
			created_at: "2024-01-01T00:00:00Z",
		};

		expect(session.id).toBe("session-1");
		expect(session.session_type).toBe("TRANSACTION_SIGNING");
	});

	it("CommandResult handles success and error", () => {
		const success: CommandResult = {
			success: true,
			message: "OK",
			data: { value: 42 },
		};

		const failure: CommandResult = {
			success: false,
			message: "Failed",
			error: { code: "NOT_FOUND", message: "Resource not found" },
		};

		expect(success.success).toBe(true);
		expect(success.data).toEqual({ value: 42 });
		expect(failure.error?.code).toBe("NOT_FOUND");
	});

	it("WindowApplicationRoute covers all routes", () => {
		const routes: WindowApplicationRoute[] = [
			"Loading",
			"Welcome",
			"SelectEnv",
			"Login",
			"MainPage",
			"MachineRegistration",
			"RemoteSessions",
			"SessionDetails",
			"LocalWallets",
		];

		expect(routes).toHaveLength(9);
	});
});
