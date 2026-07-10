import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge Tailwind class lists, resolving conflicts so the last wins. */
export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}
