import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { emit, listen } from "@tauri-apps/api/event";
import { event } from "@tauri-apps/api";

type EventPayload = {
  // Define the structure of your event payload here
};

export function useWalletConnection(message: string): {
  connection: string | null;
  initialized: boolean;
  waitingForConnection: boolean;
} {
  const [waitingForConnection, setWaitingForConnection] = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [connection, setConnection] = useState<string | null>(null);

  useEffect(() => {
    if (!waitingForConnection || !initialized) {
      invoke("start_wallet_connection", { message: message }).then((resp) => {
        console.log("message sent and received", resp)
        setWaitingForConnection(true);
      });
    }

    const unlistenPromise = listen<string>("wallet_connection", (event) => {
      console.log("Received event:", event.payload);
      setInitialized(true);
      setConnection(event.payload);
    });

    return () => {
      unlistenPromise.then((unlisten) => {
        console.log("yoyoy unlistening", unlisten);
        unlisten();
      });
    };
  }, [waitingForConnection, initialized, connection, message]);

  return { connection, initialized, waitingForConnection };
}
