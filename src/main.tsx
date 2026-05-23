import "./polyfills";
import React from "react";
import ReactDOM from "react-dom/client";
import "./globals.css";
import App from "./App";

document.addEventListener("contextmenu", (e) => e.preventDefault());
document.addEventListener("keydown", (e) => {
	if (
		e.key === "F5" ||
		(e.ctrlKey && e.key === "r") ||
		(e.ctrlKey && e.key === "R") ||
		(e.altKey && e.key === "ArrowLeft") ||
		(e.altKey && e.key === "ArrowRight")
	) {
		e.preventDefault();
	}
});

ReactDOM.createRoot(document.getElementById("root")!).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>,
);
