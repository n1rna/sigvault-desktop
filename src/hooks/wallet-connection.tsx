import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { supabase } from "@/lib/supabase-client";
import { websocket__handleCommand } from "@/lib/websocket";

type EventPayload = {
  // Define the structure of your event payload here
  success: boolean,
  error: string,
  message: {
    message_type: string;
    payload: any;
  }
};

const SocketSuccessMessages = ["Websocket connection already started", "Websocket connection started"];

export function useWalletConnection(): {
  socketConnected: boolean;
  receivedMessages: EventPayload[];
  retrySocketConnection: () => void;
} {

  const [socketConnected, setSocketConnected] = useState(false);
  const [socketConnectionRetries, setSocketConnectionRetries] = useState<number>(0);
  const [receivedMessages, setReceivedMessages] = useState<EventPayload[]>([]);

  useEffect(() => {
    if (!socketConnected) {
      supabase.auth.getSession().then(({ data: { session } }) => {
        invoke<string>("start_websocket_connection_command", { authSession: JSON.stringify(session) }).then((resp) => {
          if (SocketSuccessMessages.indexOf(resp) > -1) {
            setSocketConnected(true);
          }
        });
      });
    }

    if (socketConnected) {
      const unlistenPromise = listen<string>("websocket_connection", (event) => {
        const _message = JSON.parse(event.payload) as EventPayload;
        setReceivedMessages((prev) => [...prev, _message]);
        if (_message.message.message_type === "connection_closed") {
          setSocketConnected(false);

        } else {
          websocket__handleCommand(_message.message.message_type, _message.message.payload);
        }
      });

      return () => {
        unlistenPromise.then((unlisten) => {
          console.log("unlistening");
          unlisten();
        });
      };
    }
  }, [socketConnected, socketConnectionRetries]);

  return {
    socketConnected, receivedMessages, retrySocketConnection: () => {
      setSocketConnectionRetries((prev) => prev + 1)
    }
  };
}
