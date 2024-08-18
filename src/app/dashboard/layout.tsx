"use client";

import { Inter } from "next/font/google";
import { BackendProvider } from "@/lib/providers/backend-provider";
import { AppStateProvider } from "@/lib/providers/appstate-provider";

const inter = Inter({ subsets: ["latin"] });

export default function RootLayout({
    children,
}: Readonly<{
    children: React.ReactNode;
}>) {
    return (
        <BackendProvider>
            <AppStateProvider>
                {children}
            </AppStateProvider>
        </BackendProvider>
    );
}
