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
		<div className="page-layout">
			{(showBackButton || title) && (
				<div className="page-header">
					{showBackButton && (
						<button type="button" className="back-button" onClick={handleBack}>
							← Back
						</button>
					)}
					{title && <h1 className="page-title">{title}</h1>}
				</div>
			)}
			<div className="page-content">{children}</div>
		</div>
	);
}
