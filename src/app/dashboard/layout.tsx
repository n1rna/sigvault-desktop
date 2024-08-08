"use client";

import { Inter } from "next/font/google";
import { SocketProvider } from "@/components/providers/socket-provider";
import { AppStateProvider } from "@/components/providers/appstate-provider";

const inter = Inter({ subsets: ["latin"] });

export default function RootLayout({
    children,
}: Readonly<{
    children: React.ReactNode;
}>) {
    return (
        <SocketProvider>
            <AppStateProvider>
                {children}
            </AppStateProvider>
        </SocketProvider>
    );
}
