"use client";

import { useEffect, useState } from "react";
import { useAppState } from "@/lib/providers";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ShadowNoneIcon, LockClosedIcon, PersonIcon, LockOpen2Icon, ReloadIcon } from "@radix-ui/react-icons";
import { BackendCommandResult } from "@/lib/types";

interface RemoteSession {
    id: string;
    name: string;
    status: string;
}

export default function SessionsPage() {
    const { actionPayload } = useAppState();
    const [sessions, setSessions] = useState<RemoteSession[]>([]);
    const [loadingSessions, setLoadingSessions] = useState<boolean>(false);

    useEffect(() => {
        if (actionPayload && actionPayload.payload && actionPayload.payload.sessions) {
            setSessions(actionPayload.payload.sessions);
        }
    }, [actionPayload]);

    const handleConnectSession = (sessionId: string) => {
        // Implement the logic to connect to a session
        console.log(`Connecting to session: ${sessionId}`);
    };

    const handleRefetchSessions = (event: React.MouseEvent<HTMLButtonElement>) => {
        event.preventDefault();
        import('@tauri-apps/api').then((tauri) => {
            tauri.invoke<BackendCommandResult>("cmd_update_remote_sessions").then((resp) => {
                if (resp.success) {
                    console.log("remote sessions updated successfully", resp);
                } else {
                    console.error("Error fetching sessions", resp);
                }
            });
        });
    };

    return (
        <div className="container mx-auto p-4">
            <div className="flex items-center mb-4 gap-4">
                <h1 className="text-2xl font-bold">Remote Sessions</h1>
                {!loadingSessions ? (
                    <button onClick={handleRefetchSessions} className="flex items-center gap-2 text-sm text-muted-foreground">
                        <ReloadIcon className="w-4 h-4" />
                        <span>Reload</span>
                    </button>
                ) : (
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <span>Loading ...</span>
                    </div>
                )}
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {sessions.map((session) => (
                    <Card key={session.id}>
                        <CardHeader>
                            <CardTitle>{session.name}</CardTitle>
                        </CardHeader>
                        <CardContent>
                            <p>Status: {session.status}</p>
                            <Button
                                className="mt-2"
                                onClick={() => handleConnectSession(session.id)}
                            >
                                Connect
                            </Button>
                        </CardContent>
                    </Card>
                ))}
            </div>
        </div>
    );
}