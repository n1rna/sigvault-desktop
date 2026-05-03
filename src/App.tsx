// Main App component with routing

import { useEffect, useState } from "react";
import {
	BrowserRouter,
	Routes,
	Route,
	Outlet,
	useNavigate,
} from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { AppStateProvider, useAppState } from "./contexts/AppStateContext";
import Navbar from "./components/Navbar";
import UpdateBanner from "./components/UpdateBanner";
import Loading from "./pages/Loading";
import ModeChooser from "./pages/ModeChooser";
import { Login } from "./pages/Login";
import SelectEnv from "./pages/SelectEnv";
import Dashboard from "./pages/Dashboard";
import MachineRegistration from "./pages/MachineRegistration";
import RemoteSessions from "./pages/RemoteSessions";
import SessionDetails from "./pages/SessionDetails";
import LocalWalletList from "./pages/local/LocalWalletList";

function AuthenticatedLayout() {
	return (
		<div className="flex h-full flex-col">
			<Navbar />
			<div className="flex-1 overflow-hidden">
				<Outlet />
			</div>
			<UpdateBanner />
		</div>
	);
}

function AppRouter() {
	const { route, authenticated, listenerReady } = useAppState();
	const navigate = useNavigate();

	// Initialize app only after event listener is ready
	useEffect(() => {
		if (!listenerReady) return;
		const initializeApp = async () => {
			await invoke("cmd_initialize_app");
		};
		initializeApp();
	}, [listenerReady]);

	// Navigate based on backend-controlled route
	useEffect(() => {
		switch (route) {
			case "Loading":
				navigate("/");
				break;
			case "ModeChooser":
				navigate("/mode-chooser");
				break;
			case "SelectEnv":
				navigate("/select-env");
				break;
			case "Login":
				navigate("/login");
				break;
			case "MainPage":
				navigate("/dashboard");
				break;
			case "MachineRegistration":
				navigate("/register");
				break;
			case "RemoteSessions":
				navigate("/sessions");
				break;
			case "SessionDetails":
				navigate("/session-details");
				break;
			case "LocalWallets":
				navigate("/local/wallets");
				break;
		}
	}, [route, navigate, authenticated]);

	return (
		<Routes>
			<Route path="/" element={<Loading />} />
			<Route path="/mode-chooser" element={<ModeChooser />} />
			<Route path="/select-env" element={<SelectEnv />} />
			<Route path="/login" element={<Login />} />
			<Route path="/local/wallets" element={<LocalWalletList />} />
			<Route element={<AuthenticatedLayout />}>
				<Route path="/dashboard" element={<Dashboard />} />
				<Route path="/register" element={<MachineRegistration />} />
				<Route path="/sessions" element={<RemoteSessions />} />
				<Route path="/session-details" element={<SessionDetails />} />
			</Route>
		</Routes>
	);
}

function ActivityPanel() {
	const { activityLog } = useAppState();
	const [expanded, setExpanded] = useState(false);

	if (activityLog.length === 0) return null;

	const latest = activityLog[activityLog.length - 1];
	const isLatestInProgress = latest.level !== "success";

	return (
		<div className="fixed bottom-0 left-0 right-0 z-50 border-t border-border bg-card shadow-[0_-4px_12px_rgba(0,0,0,0.15)]">
			<button
				type="button"
				onClick={() => setExpanded(!expanded)}
				className="flex w-full items-center gap-2 px-4 py-2 text-left transition-colors hover:bg-muted"
			>
				{isLatestInProgress ? (
					<svg className="h-3.5 w-3.5 shrink-0 animate-spin text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
						<path d="M21 12a9 9 0 1 1-6.219-8.56" />
					</svg>
				) : (
					<span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
						<span className="h-2 w-2 rounded-full bg-success" />
					</span>
				)}
				<span className="flex-1 truncate font-mono text-xs text-muted-foreground">
					{latest.message}
				</span>
				<svg
					className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform ${expanded ? "rotate-180" : ""}`}
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="1.5"
				>
					<path d="m18 15-6-6-6 6" />
				</svg>
			</button>

			{expanded && (
				<div className="max-h-48 overflow-y-auto border-t border-border px-4 py-2 space-y-1.5">
					{activityLog.map((item, i) => {
						const isInProgress = i === activityLog.length - 1 && item.level !== "success";
						return (
							<div key={i} className="flex items-start gap-2">
								{isInProgress ? (
									<svg className="mt-0.5 h-3.5 w-3.5 shrink-0 animate-spin text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
										<path d="M21 12a9 9 0 1 1-6.219-8.56" />
									</svg>
								) : (
									<span className="mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center">
										<span className="h-2 w-2 rounded-full bg-success" />
									</span>
								)}
								<span className={`font-mono text-xs ${isInProgress ? "text-foreground" : "text-muted-foreground"}`}>
									{item.message}
								</span>
							</div>
						);
					})}
				</div>
			)}
		</div>
	);
}

export default function App() {
	return (
		<AppStateProvider>
			<BrowserRouter>
				<AppRouter />
				<ActivityPanel />
			</BrowserRouter>
		</AppStateProvider>
	);
}
