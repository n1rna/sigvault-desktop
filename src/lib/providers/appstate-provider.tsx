"use client";

import * as React from "react";

import { BackendContextType } from "@/lib/providers/backend-provider";
import { useBackend } from "@/lib/providers";
import { useRouter } from "next/navigation";


enum ApplicationStateRoute {
    Loading = "Loading",
    MainPage = "MainPage",
    MachineRegistration = "MachineRegistration",
    SessionDetails = "SessionDetails",
}

interface SessionState {
    step: string;
    requirements?: any;
    lastError?: string;
}


type ApplicationState = {
    route?: ApplicationStateRoute,
    socket_connected?: boolean,
    current_session_id?: string,
    session_state?: SessionState
}


type AppStateContextType = {
    socket: BackendContextType,
    actionCommand: string;
    actionPayload: any;
    applicationState: ApplicationState;
};

export const AppStateContext = React.createContext<AppStateContextType>({
    socket: {
        receivedMessages: [],
        tryBackendConnection: () => { },
        instantiated: false,
        backendAuthenticated: false,
    },
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
    "SessionMessage"
]


export const AppStateProvider = ({ children }: AppStateProviderProps) => {
    const backendSocket = useBackend();
    const [actionCommand, setActionCommand] = React.useState<string>("welcome");
    const [actionPayload, setActionPayload] = React.useState<any>({});
    const [applicationState, setApplicationState] = React.useState<ApplicationState>({
        route: ApplicationStateRoute.MainPage,
        socket_connected: false,
        current_session_id: "",
        session_state: undefined
    });
    const router = useRouter();

    React.useEffect(() => {
        if (!backendSocket.receivedMessages?.length) {
            console.log("No messages received", backendSocket.receivedMessages);
            return;
        }

        // handle messages
        const lastMessage = backendSocket.receivedMessages[0];

        if (!SupportedMessageTypes.includes(lastMessage.message.message_type)) {
            console.log("Unsupported message type: ", lastMessage.message.message_type);
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
                setActionPayload(lastMessage.message.payload);
                router.push("/dashboard/sessions");
            }
        }

        // Handle Application State Updates
        if (lastMessage.message.message_type === "SetApplicationState") {
            const lastMessagePayload = lastMessage.message.payload;
            setApplicationState(prevState => ({
                ...prevState,
                route: lastMessagePayload.route ?? prevState.route,
                socket_connected: lastMessagePayload.socket_connected ?? prevState.socket_connected,
                current_session_id: lastMessagePayload.current_session_id ?? prevState.current_session_id
            }));
        }


        // Handle Session Messages
        if (lastMessage.message.message_type === "SessionMessage") {
            const sessionPayload = lastMessage.message.payload;
            setApplicationState(prevState => ({
                ...prevState,
                session_state: {
                    step: sessionPayload.step,
                    requirements: sessionPayload.requirements,
                    lastError: sessionPayload.error
                }
            }));

            // If session completed, redirect after a short delay
            if (sessionPayload.step === "Completed") {
                setTimeout(() => {
                    router.push("/dashboard/sessions");
                }, 1500);
            }
        }

        // Handle Websocket Messages
        // if (lastMessage.message.message_type === "create_new_device") {
        //     router.push("/dashboard/devices");
        // }
        // else if (lastMessage.message.message_type === "device_created") {
        //     router.push("/dashboard/home");
        // }

    }, [backendSocket.receivedMessages, router])


    React.useEffect(() => {
        if (applicationState.route === ApplicationStateRoute.MainPage) {
            router.push("/dashboard/home");
        }
        else if (applicationState.route === ApplicationStateRoute.MachineRegistration) {
            router.push("/dashboard/register");
        } else if (applicationState.route === ApplicationStateRoute.SessionDetails) {
            router.push("/dashboard/session-details");
        }
    }, [applicationState, router])

    return (
        <AppStateContext.Provider value={{
            socket: backendSocket,
            actionCommand,
            actionPayload,
            applicationState
        }}>
            {children}
        </AppStateContext.Provider>
    );
};
