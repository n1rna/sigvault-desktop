import { useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

export default function WindowControls() {
	const onDrag = useCallback((e: React.MouseEvent) => {
		if (e.buttons === 1 && e.detail === 1) {
			e.preventDefault();
			appWindow.startDragging();
		}
	}, []);

	return (
		<div
			onMouseDown={onDrag}
			className="fixed top-0 left-0 right-0 z-50 flex h-8 select-none items-center justify-between"
		>
			<div className="flex-1" />
			<div className="flex h-full">
				<button
					type="button"
					onMouseDown={(e) => e.stopPropagation()}
					onClick={() => appWindow.minimize()}
					className="inline-flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
				>
					<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
						<path d="M5 12h14" />
					</svg>
				</button>
				<button
					type="button"
					onMouseDown={(e) => e.stopPropagation()}
					onClick={() => appWindow.close()}
					className="inline-flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
				>
					<svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
						<path d="M18 6 6 18" />
						<path d="m6 6 12 12" />
					</svg>
				</button>
			</div>
		</div>
	);
}
