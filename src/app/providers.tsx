"use client";

import ToastProvider from "@/components/toast-provider";

export function Providers({ children }: { children: React.ReactNode }) {
  return <ToastProvider>{children}</ToastProvider>;
}
