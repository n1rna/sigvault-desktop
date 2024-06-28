import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { emit, listen } from "@tauri-apps/api/event";
import { event } from "@tauri-apps/api";
import { supabase } from "@/lib/supabase-client";

type EventPayload = {
  // Define the structure of your event payload here
};

export function useWalletConnection(): {
  connection: string | null;
  initialized: boolean;
  waitingForConnection: boolean;
} {

  const [waitingForConnection, setWaitingForConnection] = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [connection, setConnection] = useState<string | null>(null);

  useEffect(() => {
    if (!waitingForConnection || !initialized) {
      supabase.auth.getSession().then(({ data: { session } }) => {
        console.log("fofofofofofo", session)
        invoke("start_websocket_connection_command", { authSession: JSON.stringify(session) }).then((resp) => {
          console.log("message sent and received", resp)
          setWaitingForConnection(true);
        });
      });
    }

    const unlistenPromise = listen<string>("websocket_connection_established", (event) => {
      console.log("Received event:", event.payload);

      if (event.payload.includes("fetched ")) {
        setInitialized(true);
        setConnection(event.payload);
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => {
        console.log("yoyoy unlistening", unlisten);
        unlisten();
      });
    };
  }, [waitingForConnection, initialized, connection]);

  return { connection, initialized, waitingForConnection };
}
