"use client";

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { supabase } from "@/lib/supabase-client";
import usePersist from "@/lib/hooks/use-persist";

import { BackendCommandResult, EventPayload } from "@/lib/types";

export function useBackendConnection(): {
  receivedMessages: EventPayload[];
  tryBackendConnection: () => void;
} {

  const [receivedMessages, setReceivedMessages] = usePersist<EventPayload[]>({ name: "receivedMessages", value: [] });

  const tryBackendConnection = useCallback(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      invoke<BackendCommandResult>("cmd_start_websocket_connection", { authSession: JSON.stringify(session) }).then((resp) => {
        console.log("cmd_start_websocket_connection", resp);
      });
    });
  }, []);

  console.log("receivedMessages", receivedMessages);

  useEffect(() => {
    const unlistenPromise = listen<string>("backend_connection", (event) => {
      const _message = JSON.parse(event.payload) as EventPayload;
      setReceivedMessages((prev) => [_message, ...prev]);
    });

    tryBackendConnection();

    return () => {
      unlistenPromise.then((unlisten) => {
        console.log("unlistening");
        unlisten();
      });
    };
  }, [setReceivedMessages, tryBackendConnection]);

  return {
    receivedMessages, tryBackendConnection
  };
}
