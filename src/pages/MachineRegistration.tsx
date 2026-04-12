import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";

export default function MachineRegistration() {
	const [machineName, setMachineName] = useState("");
	const [machineId, setMachineId] = useState("");
	const [machineType, setMachineType] = useState("");
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState("");

	useEffect(() => {
		const urlParams = new URLSearchParams(window.location.search);
		const idParam = urlParams.get("machine_id");
		const typeParam = urlParams.get("machine_type");

		if (idParam) setMachineId(idParam);
		if (typeParam) setMachineType(typeParam);
	}, []);

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		setLoading(true);
		setError("");

		try {
			const result = await invoke<CommandResult>("cmd_register_new_machine", {
				machineId,
				machineName,
				machineType,
			});

			if (!result.success) {
				setError(result.message || "Registration failed");
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to register machine. Please try again.");
		} finally {
			setLoading(false);
		}
	};

	return (
		<div className="flex h-full w-full flex-col overflow-hidden p-8">
			<div className="mx-auto max-w-[500px]">
				<h1 className="mb-2 text-2xl font-semibold text-foreground">
					Machine Registration
				</h1>
				<p className="mb-8 text-muted-foreground">
					Register this machine to continue
				</p>

				{error && (
					<div className="mb-4 border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
						{error}
					</div>
				)}

				<form onSubmit={handleSubmit} className="flex flex-col gap-4">
					<div>
						<label className="mb-2 block text-sm font-medium text-muted-foreground">
							Machine Name
						</label>
						<input
							type="text"
							placeholder="Enter a name for this machine"
							value={machineName}
							onChange={(e) => setMachineName(e.target.value)}
							disabled={loading}
							required
							className="w-full border border-border bg-input px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
						/>
					</div>

					{machineId && (
						<div>
							<label className="mb-2 block text-sm font-medium text-muted-foreground">
								Machine ID
							</label>
							<input
								type="text"
								value={machineId}
								disabled
								className="w-full border border-border bg-input px-3 py-2.5 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
							/>
						</div>
					)}

					{machineType && (
						<div>
							<label className="mb-2 block text-sm font-medium text-muted-foreground">
								Machine Type
							</label>
							<input
								type="text"
								value={machineType}
								disabled
								className="w-full border border-border bg-input px-3 py-2.5 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
							/>
						</div>
					)}

					<button
						type="submit"
						disabled={loading || !machineName}
						className="bg-primary px-5 py-2.5 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
					>
						{loading ? "Registering..." : "Register Machine"}
					</button>
				</form>
			</div>
		</div>
	);
}
