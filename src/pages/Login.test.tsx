import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Login } from "./Login";

const EMPTY_ENV_RESPONSE = { environments: [], selected_id: null };

type AuthBehavior =
	| { kind: "resolve"; value?: unknown }
	| { kind: "reject"; error: unknown }
	| { kind: "pending"; capture: (resolve: () => void) => void };

function setupInvoke(behaviors: AuthBehavior[]) {
	const queue = [...behaviors];
	vi.mocked(invoke).mockImplementation(((cmd: string) => {
		if (cmd === "cmd_list_environments") {
			return Promise.resolve(EMPTY_ENV_RESPONSE);
		}
		if (cmd === "cmd_authenticate") {
			const next = queue.shift();
			if (!next) return Promise.resolve(undefined);
			if (next.kind === "resolve") return Promise.resolve(next.value);
			if (next.kind === "reject") return Promise.reject(next.error);
			return new Promise<void>((resolve) => {
				next.capture(resolve);
			});
		}
		return Promise.resolve(undefined);
	}) as unknown as typeof invoke);
}

describe("Login", () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
		setupInvoke([]);
	});

	it("renders login page with heading and button", () => {
		render(<Login />);
		expect(screen.getByText("Welcome back")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Continue with SigVault/i })).toBeInTheDocument();
	});

	it("calls cmd_authenticate on login click", async () => {
		setupInvoke([{ kind: "resolve" }]);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		expect(invoke).toHaveBeenCalledWith("cmd_authenticate");
	});

	it("shows loading state during authentication", async () => {
		let resolveAuth: () => void = () => {};
		setupInvoke([
			{
				kind: "pending",
				capture: (r) => {
					resolveAuth = r;
				},
			},
		]);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		expect(screen.getByText("Opening browser…")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /Opening browser/i })).toBeDisabled();

		resolveAuth();
		await waitFor(() => {
			expect(screen.getByRole("button", { name: /Continue with SigVault/i })).toBeInTheDocument();
		});
	});

	it("displays error on authentication failure", async () => {
		setupInvoke([{ kind: "reject", error: new Error("Network error") }]);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		await waitFor(() => {
			expect(screen.getByText("Network error")).toBeInTheDocument();
		});
	});

	it("displays fallback error for non-Error rejections", async () => {
		setupInvoke([{ kind: "reject", error: "something went wrong" }]);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		await waitFor(() => {
			expect(screen.getByText("Failed to authenticate")).toBeInTheDocument();
		});
	});

	it("clears error on retry", async () => {
		setupInvoke([{ kind: "reject", error: new Error("First error") }, { kind: "resolve" }]);
		const user = userEvent.setup();

		render(<Login />);
		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		await waitFor(() => {
			expect(screen.getByText("First error")).toBeInTheDocument();
		});

		await user.click(screen.getByRole("button", { name: /Continue with SigVault/i }));

		await waitFor(() => {
			expect(screen.queryByText("First error")).not.toBeInTheDocument();
		});
	});
});
