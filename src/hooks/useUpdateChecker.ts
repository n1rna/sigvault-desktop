import { useState, useEffect, useCallback } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface UpdateState {
	update: Update | null;
	checking: boolean;
	downloading: boolean;
	progress: number;
	readyToRestart: boolean;
	dismissed: boolean;
	error: string | null;
}

export function useUpdateChecker() {
	const [state, setState] = useState<UpdateState>({
		update: null,
		checking: false,
		downloading: false,
		progress: 0,
		readyToRestart: false,
		dismissed: false,
		error: null,
	});

	useEffect(() => {
		let cancelled = false;

		const checkForUpdate = async () => {
			setState((s) => ({ ...s, checking: true }));
			try {
				const update = await check();
				if (!cancelled && update) {
					setState((s) => ({ ...s, update, checking: false }));
				} else if (!cancelled) {
					setState((s) => ({ ...s, checking: false }));
				}
			} catch {
				if (!cancelled) {
					setState((s) => ({ ...s, checking: false }));
				}
			}
		};

		checkForUpdate();
		return () => { cancelled = true; };
	}, []);

	const startUpdate = useCallback(async () => {
		if (!state.update) return;

		setState((s) => ({ ...s, downloading: true, progress: 0, error: null }));

		let totalSize = 0;
		let downloadedSize = 0;

		try {
			await state.update.downloadAndInstall((event) => {
				switch (event.event) {
					case "Started":
						totalSize = event.data.contentLength ?? 0;
						break;
					case "Progress":
						downloadedSize += event.data.chunkLength;
						if (totalSize > 0) {
							setState((s) => ({
								...s,
								progress: Math.round((downloadedSize / totalSize) * 100),
							}));
						}
						break;
					case "Finished":
						setState((s) => ({
							...s,
							downloading: false,
							readyToRestart: true,
							progress: 100,
						}));
						break;
				}
			});
		} catch (err) {
			setState((s) => ({
				...s,
				downloading: false,
				error: err instanceof Error ? err.message : String(err),
			}));
		}
	}, [state.update]);

	const restartApp = useCallback(async () => {
		await relaunch();
	}, []);

	const dismiss = useCallback(() => {
		setState((s) => ({ ...s, dismissed: true }));
	}, []);

	return { ...state, startUpdate, restartApp, dismiss };
}
