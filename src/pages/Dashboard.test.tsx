import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
	});

	it("renders dashboard with user info", async () => {
		vi.mocked(invoke).mockResolvedValueOnce({
			name: "Alice",
			email: "alice@example.com",
		});

		render(<Dashboard />);

		await waitFor(() => {
			expect(screen.getByText("Alice")).toBeInTheDocument();
		});
	});

	it("shows loading state initially", () => {
		vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

		render(<Dashboard />);

		expect(screen.getByText(/Join a signing session/i)).toBeInTheDocument();
	});

	it("handles navigation button clicks", async () => {
		vi.mocked(invoke).mockResolvedValueOnce({ name: "Alice" });
		const user = userEvent.setup();

		render(<Dashboard />);

		await waitFor(() => {
			expect(screen.getByText("Alice")).toBeInTheDocument();
		});

		const sessionsBtn = screen.getByRole("button", {
			name: /Join a signing session/i,
		});
		await user.click(sessionsBtn);

		expect(invoke).toHaveBeenCalledWith("cmd_navigate", {
			route: "RemoteSessions",
		});
	});
});
