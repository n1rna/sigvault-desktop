"use client";

import * as React from "react";
import { useAppState } from "@/lib/providers";

export default function DashboardPage() {

    const { applicationState: { socket_connected } } = useAppState();

    return (
        <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
            {!socket_connected && (
                <h2>Connection failed!</h2>
            )}
            {socket_connected && (
                <h2>SOOOOOOOOOCKET</h2>
            )}
        </main>
    );
}
