import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button } from "../components/ui/button";
import type { CommandResult } from "../types/events";

const VALUE_POINTS = [
	"Recognizable across multiple machines",
	"Bound to this device's identifier",
	"Editable later in the web dashboard",
];

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
			setError(
				err instanceof Error ? err.message : "Failed to register machine. Please try again.",
			);
		} finally {
			setLoading(false);
		}
	};

	return (
		<div className="flex h-full w-full overflow-hidden">
			{/* ═══ Left column: intro ═══ */}
			<section className="hidden flex-1 flex-col justify-center px-12 py-10 sm:flex">
				<h1 className="max-w-[440px] text-[28px] font-semibold leading-tight tracking-tight text-foreground text-balance">
					Give this device a name you'll recognize
				</h1>
				<p className="mt-4 max-w-[420px] text-[13px] leading-relaxed text-muted-foreground">
					SigVault ties signing privileges to the physical machine running the desktop app.
					Registering this one unlocks the remote signing workflow for your account.
				</p>
				<ul className="mt-7 space-y-2.5">
					{VALUE_POINTS.map((label) => (
						<li key={label} className="flex items-center gap-2.5 text-[13px] text-muted-foreground">
							<svg
								className="h-3.5 w-3.5 shrink-0 text-primary"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								strokeWidth="2.5"
								strokeLinecap="round"
								strokeLinejoin="round"
							>
								<path d="M20 6 9 17l-5-5" />
							</svg>
							{label}
						</li>
					))}
				</ul>
			</section>

			{/* ═══ Right column: form ═══ */}
			<aside className="flex w-full flex-col justify-center border-l border-border bg-card/40 px-10 py-10 sm:w-[460px]">
				<h2 className="text-[18px] font-semibold tracking-tight text-foreground">
					Machine details
				</h2>

				{error && (
					<div className="mt-5 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
						<svg
							className="mt-0.5 h-3.5 w-3.5 shrink-0"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<circle cx="12" cy="12" r="10" />
							<line x1="12" y1="8" x2="12" y2="12" />
							<line x1="12" y1="16" x2="12.01" y2="16" />
						</svg>
						<span className="leading-snug">{error}</span>
					</div>
				)}

				<form onSubmit={handleSubmit} className="mt-6 flex flex-col gap-5">
					<div>
						<label
							htmlFor="machine-name"
							className="mb-2 block text-[12px] font-medium text-muted-foreground"
						>
							Machine name
						</label>
						<input
							id="machine-name"
							type="text"
							placeholder="e.g. Nima's MacBook Pro"
							value={machineName}
							onChange={(e) => setMachineName(e.target.value)}
							disabled={loading}
							required
							className="h-11 w-full rounded-md border border-border bg-background px-3.5 text-[13px] text-foreground placeholder:text-muted-foreground/50 outline-none transition-colors focus:border-primary disabled:cursor-not-allowed disabled:opacity-50"
						/>
					</div>

					{machineId && (
						<div>
							<div className="mb-2 flex items-center justify-between text-[12px] font-medium text-muted-foreground">
								<span>Machine ID</span>
								<span className="text-muted-foreground/50">read-only</span>
							</div>
							<div className="flex h-11 w-full items-center rounded-md border border-border bg-muted/30 px-3.5 font-mono text-[11px] tabular-nums text-muted-foreground">
								{machineId}
							</div>
						</div>
					)}

					{machineType && (
						<div>
							<div className="mb-2 flex items-center justify-between text-[12px] font-medium text-muted-foreground">
								<span>Machine type</span>
								<span className="text-muted-foreground/50">detected</span>
							</div>
							<div className="flex h-11 w-full items-center rounded-md border border-border bg-muted/30 px-3.5 font-mono text-[11px] text-muted-foreground">
								{machineType}
							</div>
						</div>
					)}

					<Button
						type="submit"
						size="lg"
						className="mt-2 h-12 w-full"
						disabled={loading || !machineName}
					>
						{loading ? (
							<>
								<svg
									className="animate-spin"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2.5"
									strokeLinecap="round"
								>
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Registering…
							</>
						) : (
							<>
								Register machine
								<svg
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									strokeWidth="2"
									strokeLinecap="round"
									strokeLinejoin="round"
								>
									<path d="M5 12h14" />
									<path d="m12 5 7 7-7 7" />
								</svg>
							</>
						)}
					</Button>

					<p className="text-center text-[12px] text-muted-foreground/70">
						You can rename this later
					</p>
				</form>
			</aside>
		</div>
	);
}
