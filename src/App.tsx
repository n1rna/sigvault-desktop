// Main App component with routing

import { useEffect } from "react";
import { BrowserRouter, Routes, Route, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { AppStateProvider, useAppState } from "./contexts/AppStateContext";
import Loading from "./pages/Loading";
import { Login } from "./pages/Login";
import Dashboard from "./pages/Dashboard";
import MachineRegistration from "./pages/MachineRegistration";
import RemoteSessions from "./pages/RemoteSessions";
import SessionDetails from "./pages/SessionDetails";
import "./App.css";

function AppRouter() {
	const { route, authenticated } = useAppState();
	const navigate = useNavigate();

	// Initialize app on mount
	useEffect(() => {
		invoke("cmd_initialize_app")
			.then(() => console.log("App initialized"))
			.catch((err) => console.error("Initialization error:", err));
	}, []);

	// Navigate based on backend-controlled route
	useEffect(() => {
		console.log("Route changed to:", route);

		if (!authenticated && route !== "Login" && route !== "Loading") {
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
			<Route path="/dashboard" element={<Dashboard />} />
			<Route path="/register" element={<MachineRegistration />} />
			<Route path="/sessions" element={<RemoteSessions />} />
			<Route path="/session-details" element={<SessionDetails />} />
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
