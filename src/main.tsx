import { Buffer } from "buffer";
import React from "react";
import ReactDOM from "react-dom/client";
import "./globals.css";
import App from "./App";

// `@ngraveio/bc-ur` (UR codec used by the QBL-234 animated-PSBT QR
// flow) calls Buffer.from / Buffer.alloc unprefixed, expecting Node's
// global Buffer. Vite doesn't shim that automatically — install it on
// window before any module that uses UR loads.
if (typeof window !== "undefined" && !(window as unknown as { Buffer?: unknown }).Buffer) {
	(window as unknown as { Buffer: typeof Buffer }).Buffer = Buffer;
}

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
