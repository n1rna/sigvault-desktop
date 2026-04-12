import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { Login } from "./Login";

vi.mock("../components/WindowControls", () => ({
	default: () => <div data-testid="window-controls" />,
}));

describe("Login", () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
	});

	it("renders login page with heading and button", () => {
		render(<Login />);
		expect(screen.getByText("Welcome to SigVault")).toBeInTheDocument();
		expect(screen.getByText("Login with OAuth")).toBeInTheDocument();
	});

	it("calls cmd_authenticate on login click", async () => {
		vi.mocked(invoke).mockResolvedValueOnce(undefined);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByText("Login with OAuth"));

		expect(invoke).toHaveBeenCalledWith("cmd_authenticate");
	});

	it("shows loading state during authentication", async () => {
		let resolveAuth: () => void;
		vi.mocked(invoke).mockImplementation(
			() => new Promise<void>((resolve) => { resolveAuth = resolve; }),
		);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByText("Login with OAuth"));

		expect(screen.getByText("Connecting...")).toBeInTheDocument();
		expect(screen.getByRole("button")).toBeDisabled();

		resolveAuth!();
		await waitFor(() => {
			expect(screen.getByText("Login with OAuth")).toBeInTheDocument();
		});
	});

	it("displays error on authentication failure", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new Error("Network error"));
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByText("Login with OAuth"));

		await waitFor(() => {
			expect(screen.getByText("Network error")).toBeInTheDocument();
		});
	});

	it("displays fallback error for non-Error rejections", async () => {
		vi.mocked(invoke).mockRejectedValueOnce("something went wrong");
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByText("Login with OAuth"));

		await waitFor(() => {
			expect(screen.getByText("Failed to authenticate")).toBeInTheDocument();
		});
	});

	it("clears error on retry", async () => {
		vi.mocked(invoke)
			.mockRejectedValueOnce(new Error("First error"))
			.mockResolvedValueOnce(undefined);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByText("Login with OAuth"));

		await waitFor(() => {
			expect(screen.getByText("First error")).toBeInTheDocument();
		});

		await user.click(screen.getByText("Login with OAuth"));

		await waitFor(() => {
			expect(screen.queryByText("First error")).not.toBeInTheDocument();
		});
	});
});
