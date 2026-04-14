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
		<div className="relative flex h-full w-full overflow-hidden">
			{/* ── Ambient background ── */}
			<div className="pointer-events-none absolute inset-0 bg-dots mask-radial-fade opacity-[0.06]" />
			<div className="pointer-events-none absolute -left-40 top-1/4 h-[420px] w-[420px] rounded-full bg-primary/[0.05] blur-[140px]" />
			<div className="pointer-events-none absolute -right-32 bottom-0 h-[320px] w-[320px] rounded-full bg-accent/[0.05] blur-[120px]" />

			{/* ═══ Left column: hero ═══ */}
			<section className="relative flex flex-1 flex-col justify-center px-12 py-10">
				<div className="flex items-center gap-2.5 font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					<span className="h-px w-6 bg-primary/60" />
					§ 01 — Register machine
				</div>

				<h1 className="mt-5 max-w-[440px] text-[36px] font-medium leading-[1.1] tracking-[-0.02em] text-foreground">
					Give this device
					<br />
					<span className="text-primary">a name you'll recognize.</span>
				</h1>

				<p className="mt-5 max-w-[420px] text-[13px] leading-relaxed text-muted-foreground">
					SigVault ties signing privileges to the physical machine running
					the desktop app. Registering this one unlocks the remote signing
					workflow for your account.
				</p>

				<ul className="mt-8 space-y-3">
					{[
						"Recognizable across multiple machines",
						"Bound to this device's identifier",
						"Editable later in web dashboard",
					].map((label) => (
						<li
							key={label}
							className="flex items-center gap-3 font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground"
						>
							<span className="h-px w-5 bg-primary/50" />
							{label}
						</li>
					))}
				</ul>
			</section>

			{/* ═══ Right column: form ═══ */}
			<aside className="relative flex w-[460px] flex-col justify-center border-l border-border/80 bg-card/40 px-10 py-10 backdrop-blur-sm">
				<div className="pointer-events-none absolute inset-y-0 left-0 w-px bg-gradient-to-b from-transparent via-border to-transparent" />

				<div className="font-mono text-[10px] uppercase tracking-[0.22em] text-muted-foreground">
					§ Form
				</div>
				<h2 className="mt-3 text-[20px] font-medium tracking-tight text-foreground">
					Machine details
				</h2>

				{error && (
					<div className="mt-5 flex items-start gap-2.5 rounded-md border border-destructive/30 bg-destructive/[0.06] px-3.5 py-3 text-[12px] text-destructive">
						<svg className="mt-0.5 h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
							<circle cx="12" cy="12" r="10" />
							<line x1="12" y1="8" x2="12" y2="12" />
							<line x1="12" y1="16" x2="12.01" y2="16" />
						</svg>
						<span className="leading-snug">{error}</span>
					</div>
				)}

				<form onSubmit={handleSubmit} className="mt-6 flex flex-col gap-5">
					<div>
						<label className="mb-2 block font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
							Machine name
						</label>
						<input
							type="text"
							placeholder="e.g. Nima's MacBook Pro"
							value={machineName}
							onChange={(e) => setMachineName(e.target.value)}
							disabled={loading}
							required
							className="h-11 w-full rounded-md border border-border bg-background px-3.5 text-[13px] text-foreground placeholder:text-muted-foreground/50 transition-colors focus:border-primary/60 focus:bg-background focus:outline-none focus:ring-4 focus:ring-primary/[0.08] disabled:cursor-not-allowed disabled:opacity-50"
						/>
					</div>

					{machineId && (
						<div>
							<label className="mb-2 flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
								<span>Machine ID</span>
								<span className="text-muted-foreground/50">read-only</span>
							</label>
							<div className="flex h-11 w-full items-center rounded-md border border-border bg-muted/30 px-3.5 font-mono text-[11px] tabular-nums text-muted-foreground">
								{machineId}
							</div>
						</div>
					)}

					{machineType && (
						<div>
							<label className="mb-2 flex items-center justify-between font-mono text-[9px] uppercase tracking-[0.22em] text-muted-foreground">
								<span>Machine type</span>
								<span className="text-muted-foreground/50">detected</span>
							</label>
							<div className="flex h-11 w-full items-center rounded-md border border-border bg-muted/30 px-3.5 font-mono text-[11px] text-muted-foreground">
								{machineType}
							</div>
						</div>
					)}

					<button
						type="submit"
						disabled={loading || !machineName}
						className="group mt-2 flex h-12 w-full items-center justify-center gap-2.5 rounded-md bg-primary text-[13px] font-medium tracking-[0.02em] text-primary-foreground shadow-md transition-all hover:shadow-lg hover:-translate-y-[1px] disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0 disabled:hover:shadow-md"
					>
						{loading ? (
							<>
								<svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
									<path d="M21 12a9 9 0 1 1-6.219-8.56" />
								</svg>
								Registering…
							</>
						) : (
							<>
								Register machine
								<svg className="h-4 w-4 transition-transform group-hover:translate-x-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
									<path d="M5 12h14" />
									<path d="m12 5 7 7-7 7" />
								</svg>
							</>
						)}
					</button>

					<p className="text-center font-mono text-[9px] uppercase tracking-[0.16em] text-muted-foreground/70">
						you can rename this later
					</p>
				</form>
			</aside>
		</div>
	);
}
