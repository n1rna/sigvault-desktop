import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "../../lib/utils";

const badgeVariants = cva(
	"inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-medium leading-none whitespace-nowrap",
	{
		variants: {
			variant: {
				default: "bg-primary/12 text-primary",
				secondary: "bg-muted text-muted-foreground",
				outline: "border border-border text-muted-foreground",
				success: "bg-success/15 text-success",
				warning: "bg-warning/15 text-warning",
				destructive: "bg-destructive/15 text-destructive",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	},
);

export interface BadgeProps
	extends React.HTMLAttributes<HTMLSpanElement>,
		VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
	return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { badgeVariants };
