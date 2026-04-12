import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import Dashboard from "./Dashboard";

vi.mock("../hooks/useAppEvents", () => ({
	useAppState: () => ({
		state: {
			authenticated: true,
			route: "MainPage",
			listenerReady: true,
			notification: null,
			activityLog: [],
			activeSession: { isConnected: false },
			remoteSessions: [],
		},
	}),
}));

vi.mock("../components/WindowControls", () => ({
	default: () => <div data-testid="window-controls" />,
}));

vi.mock("../components/Navbar", () => ({
	default: () => <nav data-testid="navbar" />,
}));

describe("Dashboard", () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
		vi.mocked(getVersion).mockResolvedValue("1.2.3");
	});

	it("renders dashboard with user info", async () => {
		vi.mocked(invoke).mockResolvedValueOnce({
			name: "Alice",
			email: "alice@example.com",
		});

		render(<Dashboard />);

		await waitFor(() => {
			expect(screen.getByText("Welcome, Alice")).toBeInTheDocument();
		});
	});

	it("shows loading state initially", () => {
		vi.mocked(invoke).mockImplementation(
			() => new Promise(() => {}),
		);

		render(<Dashboard />);

		expect(screen.getByText("Loading…")).toBeInTheDocument();
	});

	it("displays version from Tauri API", async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ name: "Alice" });

		render(<Dashboard />);

		await waitFor(() => {
			expect(screen.getByText(/SigVault Desktop v1\.2\.3/)).toBeInTheDocument();
		});
	});

	it("handles navigation button clicks", async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ name: "Alice" });
		const user = userEvent.setup();

		render(<Dashboard />);

		await waitFor(() => {
			expect(screen.getByText("Welcome, Alice")).toBeInTheDocument();
		});

		const sessionsBtn = screen.getByText("Sessions");
		await user.click(sessionsBtn);

		expect(invoke).toHaveBeenCalledWith("cmd_navigate", {
			route: "RemoteSessions",
		});
	});
});
