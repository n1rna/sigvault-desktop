"use client";

import * as React from "react";

import { useRouter } from "next/navigation";
import { toast, Flip } from "react-toastify";
import { useBackendConnection } from "@/lib/hooks/use-backend-connection";
import { BackendCommandResult, RemoteSession } from "@/lib/types";

export type BackendContextType = {
  receivedMessages: {
    success: boolean;
    error?: string;
    message: { message_type: string; payload: any };
  }[];
  tryBackendConnection: () => void;
  instantiated: boolean;
  backendAuthenticated: boolean;
};

enum ApplicationStateRoute {
  Loading = "Loading",
  MainPage = "MainPage",
  MachineRegistration = "MachineRegistration",
  SessionDetails = "SessionDetails",
  RemoteSessions = "RemoteSessions",
}

interface SessionState {
  step: string;
  requirements?: any;
  lastError?: string;
  data?: any;
}

type ApplicationState = {
  route?: ApplicationStateRoute;
  socket_connected?: boolean;
  current_session_id?: string;
  current_session_type?: string;
  session_state?: SessionState;
};

type AppStateContextType = {
  socket: BackendContextType;
  remoteSessions: RemoteSession[];
  cleanupSession: () => void;
  actionCommand: string;
  actionPayload: any;
  applicationState: ApplicationState;
};

export const AppStateContext = React.createContext<AppStateContextType>({
  socket: {
    receivedMessages: [],
    tryBackendConnection: () => {},
    instantiated: false,
    backendAuthenticated: false,
  },
  remoteSessions: [],
  cleanupSession: () => {},
  actionCommand: "welcome",
  actionPayload: {},
  applicationState: {
    route: ApplicationStateRoute.Loading,
    socket_connected: false,
  },
});

interface AppStateProviderProps {
  children: React.ReactNode;
}

const SupportedMessageTypes = [
  "create_new_device",
  "device_created",
  "device_create_failed",
  "sign_transaction",
  "BackendCommand",
  "SetApplicationState",
  "TextMessage",
  "SessionMessage",
];

export const AppStateProvider = ({ children }: AppStateProviderProps) => {
  const {
    receivedMessages,
    tryBackendConnection,
    instantiated,
    backendAuthenticated,
  } = useBackendConnection();
  const [actionCommand, setActionCommand] = React.useState<string>("welcome");
  const [actionPayload, setActionPayload] = React.useState<any>({});
  const [remoteSessions, setRemoteSessions] = React.useState<RemoteSession[]>([]);
  const [applicationState, setApplicationState] =
    React.useState<ApplicationState>({
      route: ApplicationStateRoute.MainPage,
      socket_connected: false,
      current_session_id: "",
      current_session_type: "",
      session_state: undefined,
    });
  const router = useRouter();

  React.useEffect(() => {
    if (!receivedMessages?.length) {
      console.log("No messages received", receivedMessages);
      return;
    }

    // handle messages
    const lastMessage = receivedMessages[0];

    if (!SupportedMessageTypes.includes(lastMessage.message.message_type)) {
      console.log(
        "Unsupported message type: ",
        lastMessage.message.message_type
      );
      return;
    }

    setActionCommand(lastMessage.message.message_type);
    setActionPayload(lastMessage.message.payload);

    const lastMessagePayload = lastMessage.message.payload;

    // Handle Backend Commands
    if (lastMessage.message.message_type === "BackendCommand") {
      if (lastMessagePayload.command === "register_machine") {
        console.log("Register Machine: ", lastMessagePayload);
      }
      if (lastMessagePayload.command === "update_remote_sessions") {
        setRemoteSessions(lastMessagePayload.payload.sessions);
        // router.push("/dashboard/sessions");
      }
    }

    // Handle Application State Updates
    if (lastMessage.message.message_type === "SetApplicationState") {
      const lastMessagePayload = lastMessage.message.payload;
      setApplicationState((prevState) => ({
        ...prevState,
        route: lastMessagePayload.route ?? prevState.route,
        socket_connected:
          lastMessagePayload.socket_connected ?? prevState.socket_connected,
        current_session_id:
          lastMessagePayload.current_session_id ?? prevState.current_session_id,
        current_session_type:
          lastMessagePayload.current_session_type ??
          prevState.current_session_type,
      }));
    }

    // Handle Session Messages
    if (lastMessage.message.message_type === "SessionMessage") {
      const sessionPayload = lastMessage.message.payload;
      if (sessionPayload.message_type === "WorkflowSession") {
        if (!sessionPayload?.payload?.success) {
          toast.error(sessionPayload?.payload?.error, {
            position: "top-right",
            autoClose: 5000,
            hideProgressBar: false,
            closeOnClick: true,
            pauseOnHover: true,
            draggable: true,
            progress: undefined,
            theme: "light",
            transition: Flip,
            bodyClassName: "text-sm",
            className: "top-8",
          });
          return;
        }
        const workflowPayload = sessionPayload?.payload?.payload;
        setApplicationState((prevState) => ({
          ...prevState,
          current_session_type: workflowPayload?.type,
          session_state: {
            step: workflowPayload?.step,
            requirements: workflowPayload?.requirements,
            lastError: workflowPayload?.error,
            data: workflowPayload?.data,
          },
        }));

        // If session completed, redirect after a short delay
        if (workflowPayload?.step === "completed") {
          setTimeout(() => {
            router.push("/dashboard/sessions");
          }, 1500);
        }
      }
    }

    // Handle Websocket Messages
    // if (lastMessage.message.message_type === "create_new_device") {
    //     router.push("/dashboard/devices");
    // }
    // else if (lastMessage.message.message_type === "device_created") {
    //     router.push("/dashboard/home");
    // }
  }, [receivedMessages, router]);

  const cleanupSession = async () => {
    const tauri = await import("@tauri-apps/api/core");
    const resp = await tauri.invoke<BackendCommandResult>("cmd_exit_session");
    if (!resp.success) {
      console.error("Error exiting session", resp);
    }
    setApplicationState((prevState) => ({
      ...prevState,
      route: ApplicationStateRoute.RemoteSessions,
      current_session_id: "",
      current_session_type: "",
      session_state: undefined,
    }));
  };

  React.useEffect(() => {
    if (applicationState.route === ApplicationStateRoute.MainPage) {
      router.push("/dashboard");
    } else if (
      applicationState.route === ApplicationStateRoute.MachineRegistration
    ) {
      router.push("/dashboard/register");
    } else if (
      applicationState.route === ApplicationStateRoute.SessionDetails
    ) {
      router.push("/dashboard/session-details");
    } else if (
      applicationState.route === ApplicationStateRoute.RemoteSessions
    ) {
      router.push("/dashboard/sessions");
    }
  }, [applicationState, router]);

  return (
    <AppStateContext.Provider
      value={{
        socket: {
          receivedMessages,
          tryBackendConnection,
          instantiated,
          backendAuthenticated,
        },
        remoteSessions,
        actionCommand,
        actionPayload,
        applicationState,
        cleanupSession,
      }}
    >
      {children}
    </AppStateContext.Provider>
  );
};
