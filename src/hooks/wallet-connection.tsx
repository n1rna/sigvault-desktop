import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { emit, listen } from "@tauri-apps/api/event";
import { Event } from "@tauri-apps/api/event";
import { event } from "@tauri-apps/api";
import { supabase } from "@/lib/supabase-client";

type EventPayload = {
  // Define the structure of your event payload here
  success: boolean,
  error: string,
  message: {
    message_type: string;
    payload: any;
  }
};

export function useWalletConnection(): {
  socketConnected: boolean;
  receivedMessages: EventPayload[];
} {

  const [socketConnected, setSocketConnected] = useState(false);
  const [receivedMessages, setReceivedMessages] = useState<EventPayload[]>([]);

  useEffect(() => {
    if (!socketConnected) {
      supabase.auth.getSession().then(({ data: { session } }) => {
        invoke("start_websocket_connection_command", { authSession: JSON.stringify(session) }).then((resp) => {
          setSocketConnected(true);
        });
      });
    }

    const unlistenPromise = listen<string>("websocket_connection", (event) => {
      setReceivedMessages((prev) => [...prev, JSON.parse(event.payload) as EventPayload]);
    });

    return () => {
      unlistenPromise.then((unlisten) => {
        console.log("unlistening");
        unlisten();
      });
    };
  }, [socketConnected]);

  return { socketConnected, receivedMessages };
}
