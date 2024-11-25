"use client";

import { useEffect, useState } from "react";
import { useAppState } from "@/lib/providers";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  ShadowNoneIcon,
  LockClosedIcon,
  PersonIcon,
  LockOpen2Icon,
  ReloadIcon,
} from "@radix-ui/react-icons";
import { BackendCommandResult } from "@/lib/types";
import { useWebSocketConnection } from "@/lib/hooks/use-websocket-connection";

interface RemoteSession {
  id: string;
  name: string;
  status: string;
  session_type: string;
}

export default function SessionsPage() {
  const { actionPayload } = useAppState();
  const { connect } = useWebSocketConnection();
  const [sessions, setSessions] = useState<RemoteSession[]>([]);
  const [loadingSessions, setLoadingSessions] = useState<boolean>(false);

  useEffect(() => {
    if (
      actionPayload &&
      actionPayload.payload &&
      actionPayload.payload.sessions
    ) {
      setSessions(actionPayload.payload.sessions);
    }
  }, [actionPayload]);

  const handleConnectSession = (sessionId: string) => {
    // Implement the logic to connect to a session
    console.log(`Connecting to session: ${sessionId}`);
    connect(sessionId);
  };

  const handleRefetchSessions = (
    event: React.MouseEvent<HTMLButtonElement>
  ) => {
    event.preventDefault();
    setLoadingSessions(true);
    import("@tauri-apps/api/core").then((tauri) => {
      tauri
        .invoke<BackendCommandResult>("cmd_update_remote_sessions")
        .then((resp) => {
          if (resp.success) {
            console.log("remote sessions updated successfully", resp);
          } else {
            console.error("Error fetching sessions", resp);
          }
          setLoadingSessions(false);
        });
    });
  };

  return (
    <main className="flex-1 overflow-auto p-6">
      <div className="mx-auto max-w-5xl">
        <div className="flex items-center justify-between">
          <h1 className="text-3xl font-semibold tracking-tight">
            Remote Sessions
          </h1>
          {!loadingSessions ? (
            <Button
              variant="outline"
              size="icon"
              onClick={handleRefetchSessions}
            >
              <ReloadIcon className="h-4 w-4" />
              <span className="sr-only">Reload</span>
            </Button>
          ) : (
            <Button variant="outline" size="icon" disabled>
              <ReloadIcon className="h-4 w-4" />
              <span className="sr-only">Loading</span>
            </Button>
          )}
        </div>

        <div className="mt-8 grid gap-4">
          {sessions.map((session) => (
            <Card key={session.id} className="p-6">
              <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <div className="space-y-1">
                  <h2 className="font-mono text-lg">{session.id}</h2>
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <span className="capitalize">{session.session_type}</span>
                  </div>
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <span>Status:</span>
                    <span className="capitalize">{session.status}</span>
                  </div>
                </div>
                {session.status === "Pending" && (
                  <Button
                    className="sm:w-[100px]"
                    onClick={() => handleConnectSession(session.id)}
                  >
                    Connect
                  </Button>
                )}
              </div>
            </Card>
          ))}
        </div>
      </div>
    </main>
  );
}
