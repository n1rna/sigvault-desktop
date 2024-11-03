"use client";

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { supabase } from "@/lib/supabase-client";
import usePersist from "@/lib/hooks/use-persist";

import { BackendCommandResult, EventPayload } from "@/lib/types";

interface MessageWithId {
  id: string;
  message: EventPayload;
}

export function useBackendConnection(): {
  receivedMessages: EventPayload[];
  tryBackendConnection: () => void;
  instantiated: boolean;
  backendAuthenticated: boolean;
} {

  const [instantiated, setInstantiated] = useState(false);
  const [backendAuthenticated, setBackendAuthenticated] = useState(false);
  const [receivedMessages, setReceivedMessages] = usePersist<EventPayload[]>({ name: "receivedMessages", value: [] });

  const tryBackendConnection = useCallback(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      invoke<BackendCommandResult>("cmd_start_backend_authentication", { authSession: JSON.stringify(session) }).then((resp) => {
        console.log("cmd_start_backend_authentication", resp);
        setBackendAuthenticated(resp.success);
      });
    });
  }, []);

  const processMessage = useCallback((messageWithId: MessageWithId) => {
    console.log("Processing message:", messageWithId);

    setReceivedMessages((prev) => prev ? [messageWithId.message as unknown as EventPayload, ...prev] : [messageWithId.message as unknown as EventPayload]);

    invoke("cmd_message_processed", { messageId: messageWithId.id }).catch((error) => {
      console.error("Error signaling message processed:", error);
    });
  }, [setReceivedMessages]);

  console.log("receivedMessages", receivedMessages);

  useEffect(() => {
    const unlistenPromise = listen<string>("backend_connection", (event) => {
      // const _message = JSON.parse(event.payload) as EventPayload;
      const messageWithId = event.payload as unknown as MessageWithId;
      // setReceivedMessages((prev) => [messageWithId, ...prev]);
      processMessage(messageWithId);
    });

    tryBackendConnection();
    setInstantiated(true);

    return () => {
      unlistenPromise.then((unlisten) => {
        console.log("unlistening");
        unlisten();
      });
    };
  }, [processMessage, tryBackendConnection]);

  return {
    receivedMessages, tryBackendConnection, instantiated, backendAuthenticated
  };
}
