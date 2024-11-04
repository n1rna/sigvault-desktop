"use client";

import * as React from "react";
import { useAppState } from "@/lib/providers";

export default function DashboardPage() {

    const { applicationState: { socket_connected, current_session_type, current_session_id, session_state } } = useAppState();

    return (
        <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
            {!socket_connected && (
                <h2>Connection failed!</h2>
            )}
            {socket_connected && (
                <>
                    <h2>session id: {current_session_id}</h2>
                    <h2>session type: {current_session_type}</h2>
                    <p>{JSON.stringify(session_state)}</p>
                </>
            )}
        </main>
    );
}
