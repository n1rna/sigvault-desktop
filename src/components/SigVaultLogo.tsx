interface SigVaultLogoProps {
	className?: string;
}

export function SigVaultLogo({ className }: SigVaultLogoProps) {
	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="0 0 102 102"
			role="img"
			aria-label="sigvault"
			className={className}
		>
			<rect x="0" y="0" width="30" height="30" fill="currentColor" />
			<rect x="36" y="0" width="30" height="30" fill="currentColor" />
			<rect x="72" y="0" width="30" height="30" fill="currentColor" />
			<rect x="0" y="36" width="30" height="30" fill="currentColor" />
			<rect x="36" y="36" width="30" height="30" fill="#F7941D" />
			<rect x="72" y="36" width="30" height="30" fill="currentColor" />
			<rect x="0" y="72" width="30" height="30" fill="currentColor" />
			<rect x="36" y="72" width="30" height="30" fill="currentColor" />
			<rect x="72" y="72" width="30" height="30" fill="currentColor" />
		</svg>
	);
}
