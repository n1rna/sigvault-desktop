import { invoke } from "@tauri-apps/api/core";
import { ReactNode } from "react";

interface PageLayoutProps {
	children: ReactNode;
	title?: string;
	showBackButton?: boolean;
	backRoute?: string;
}

export default function PageLayout({
	children,
	title,
	showBackButton = false,
	backRoute = "MainPage",
}: PageLayoutProps) {
	const handleBack = async () => {
		try {
			await invoke("cmd_navigate", { route: backRoute });
		} catch (error) {
			console.error("Failed to navigate:", error);
		}
	};

	return (
		<div className="flex h-full flex-col overflow-hidden">
			{(showBackButton || title) && (
				<div className="mb-6 flex shrink-0 items-center gap-4">
					{showBackButton && (
						<button
							type="button"
							className="border border-border bg-secondary px-4 py-2 text-sm font-medium text-foreground hover:bg-accent"
							onClick={handleBack}
						>
							&larr; Back
						</button>
					)}
					{title && (
						<h1 className="text-[1.75rem] font-semibold text-foreground">
							{title}
						</h1>
					)}
				</div>
			)}
			<div className="flex-1 overflow-y-auto overflow-x-hidden">
				{children}
			</div>
		</div>
	);
}
