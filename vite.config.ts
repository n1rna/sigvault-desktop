import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [react(), tailwindcss()],
	// prevent vite from obscuring rust errors
	clearScreen: false,
	server: {
		// make sure this port matches the devUrl port in tauri.conf.json file
		port: 1420,
		// Tauri expects a fixed port, fail if that port is not available
		strictPort: true,
		// if the host Tauri is expecting is set, use it
		host: host || false,
		hmr: host
			? {
					protocol: "ws",
					host,
					port: 1421,
				}
			: undefined,

		watch: {
			// tell vite to ignore watching `src-tauri`
			ignored: ["**/src-tauri/**"],
		},
	},
	// Env variables starting with the item of `envPrefix` will be exposed in tauri's source code through `import.meta.env`.
	envPrefix: ["VITE_", "TAURI_ENV_*"],
	build: {
		target:
			// @ts-expect-error process is a nodejs global
			process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
		// @ts-expect-error process is a nodejs global
		minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
		// produce sourcemaps for debug builds
		// @ts-expect-error process is a nodejs global
		sourcemap: !!process.env.TAURI_ENV_DEBUG,
	},
});
