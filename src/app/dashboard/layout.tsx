"use client";

import { Inter } from "next/font/google";
import { AppStateProvider } from "@/lib/providers/appstate-provider";

const inter = Inter({ subsets: ["latin"] });

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <AppStateProvider>{children}</AppStateProvider>;
}
