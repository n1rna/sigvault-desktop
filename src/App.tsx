// Main App component with routing

import { useEffect } from "react";
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
import Loading from "./pages/Loading";
import { Login } from "./pages/Login";
import Dashboard from "./pages/Dashboard";
import MachineRegistration from "./pages/MachineRegistration";
import RemoteSessions from "./pages/RemoteSessions";
import SessionDetails from "./pages/SessionDetails";

function AuthenticatedLayout() {
	return (
		<div className="flex h-full flex-col">
			<Navbar />
			<div className="flex-1 overflow-hidden">
				<Outlet />
			</div>
		</div>
	);
}

function AppRouter() {
	const { route, authenticated } = useAppState();
	const navigate = useNavigate();

	// Initialize app on mount
	useEffect(() => {
		const initializeApp = async () => {
			await invoke("cmd_initialize_app");
			console.log("App initialized");
		};
		initializeApp();
	}, []);

	// Navigate based on backend-controlled route
	useEffect(() => {
		console.log("Route changed to:", route);

		if (!authenticated && route !== "Login") {
			navigate("/login");
		}

		switch (route) {
			case "Loading":
				navigate("/");
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
		}
	}, [route, navigate, authenticated]);

	return (
		<Routes>
			<Route path="/" element={<Loading />} />
			<Route path="/login" element={<Login />} />
			<Route element={<AuthenticatedLayout />}>
				<Route path="/dashboard" element={<Dashboard />} />
				<Route path="/register" element={<MachineRegistration />} />
				<Route path="/sessions" element={<RemoteSessions />} />
				<Route path="/session-details" element={<SessionDetails />} />
			</Route>
		</Routes>
	);
}

export default function App() {
	return (
		<AppStateProvider>
			<BrowserRouter>
				<AppRouter />
			</BrowserRouter>
		</AppStateProvider>
	);
}
