import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { SocketProvider } from "@/components/socket-provider";
import "../globals.css";
import 'react-toastify/dist/ReactToastify.css';

const inter = Inter({ subsets: ["latin"] });

export const metadata: Metadata = {
    title: "Qblok Vault",
    description: "Qblok Vault by 21st Capital",
};


export default function RootLayout({
    children,
}: Readonly<{
    children: React.ReactNode;
}>) {
    return (
        <SocketProvider>
            {children}
        </SocketProvider>
    );
}
