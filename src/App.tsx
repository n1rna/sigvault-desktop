// Main App component with routing

import { useEffect, useState } from "react";
import {
	BrowserRouter,
	Routes,
	Route,
	Outlet,
	useLocation,
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
import CreateWalletWizard from "./pages/local/CreateWalletWizard";
import WalletDashboard from "./pages/local/WalletDashboard";
import ReceiveScreen from "./pages/local/ReceiveScreen";
import SendScreen from "./pages/local/SendScreen";
import LocalSettings from "./pages/local/LocalSettings";

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

// Map backend route enum → frontend routing spec.
// `target` is the default landing path. `namespace` (when set) marks a
// broader prefix that counts as "still satisfying this backend route" —
// used so siblings like /local/settings don't snap back to /local/wallets
// when the backend is asserting "you should be in local mode".
interface RouteSpec {
	target: string;
	namespace?: string;
}

const ROUTE_PATHS: Record<string, RouteSpec> = {
	Loading: { target: "/" },
	ModeChooser: { target: "/mode-chooser" },
	SelectEnv: { target: "/select-env" },
	Login: { target: "/login" },
	MainPage: { target: "/dashboard" },
	MachineRegistration: { target: "/register" },
	RemoteSessions: { target: "/sessions" },
	SessionDetails: { target: "/session-details" },
	LocalWallets: { target: "/local/wallets", namespace: "/local/" },
};

function AppRouter() {
	const { route, authenticated, listenerReady } = useAppState();
	const navigate = useNavigate();
	const location = useLocation();

	// Initialize app only after event listener is ready
	useEffect(() => {
		if (!listenerReady) return;
		const initializeApp = async () => {
			await invoke("cmd_initialize_app");
		};
		initializeApp();
	}, [listenerReady]);

	// Navigate based on backend-controlled route. Idempotent: if the user
	// has already drilled into a child route (/local/wallets/new) or a
	// sibling within the route's namespace (/local/settings), this stays
	// put rather than snapping back to the default target.
	useEffect(() => {
		const spec = ROUTE_PATHS[route];
		if (!spec) return;
		const { target, namespace } = spec;
		if (location.pathname === target) return;
		if (target !== "/" && location.pathname.startsWith(`${target}/`)) return;
		if (namespace && location.pathname.startsWith(namespace)) return;
		navigate(target);
	}, [route, navigate, authenticated, location.pathname]);

	return (
		<Routes>
			<Route path="/" element={<Loading />} />
			<Route path="/mode-chooser" element={<ModeChooser />} />
			<Route path="/select-env" element={<SelectEnv />} />
			<Route path="/login" element={<Login />} />
			<Route path="/local/wallets" element={<LocalWalletList />} />
			<Route path="/local/wallets/new" element={<CreateWalletWizard />} />
			<Route path="/local/wallets/:walletId" element={<WalletDashboard />} />
			<Route
				path="/local/wallets/:walletId/receive"
				element={<ReceiveScreen />}
			/>
			<Route path="/local/wallets/:walletId/send" element={<SendScreen />} />
			<Route path="/local/settings" element={<LocalSettings />} />
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
