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

type ApplicationState = {
    route?: ApplicationStateRoute,
    socket_connected?: boolean,
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
    "TextMessage"
]


export const AppStateProvider = ({ children }: AppStateProviderProps) => {
    const socketState = useBackend();
    const [actionCommand, setActionCommand] = React.useState<string>("welcome");
    const [actionPayload, setActionPayload] = React.useState<any>({});
    const [applicationState, setApplicationState] = React.useState<ApplicationState>({
        route: ApplicationStateRoute.MainPage,
        socket_connected: false,
    });

    const router = useRouter();

    React.useEffect(() => {
        if (!socketState.receivedMessages?.length) {
            console.log("No messages received", socketState.receivedMessages);
            return;
        }

        // handle messages
        const lastMessage = socketState.receivedMessages[0];

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

        if (lastMessage.message.message_type === "SetApplicationState") {
            console.log("Set Application State: ", lastMessagePayload);
            // setApplicationState(lastMessagePayload);
            setApplicationState(prevState => {
                const newState: ApplicationState = prevState ? { ...prevState } : {};

                // Only update fields that are present in the payload
                if ('route' in lastMessagePayload) {
                    newState.route = lastMessagePayload.route;
                }
                if ('socket_connected' in lastMessagePayload) {
                    newState.socket_connected = lastMessagePayload.socket_connected;
                }
                // Add other fields as needed
                return newState;
            });
        }

        // Handle Websocket Messages
        if (lastMessage.message.message_type === "create_new_device") {
            router.push("/dashboard/devices");
        }
        else if (lastMessage.message.message_type === "device_created") {
            router.push("/dashboard/home");
        }

    }, [socketState.receivedMessages, router])


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
            socket: socketState,
            actionCommand,
            actionPayload,
            applicationState
        }}>
            {children}
        </AppStateContext.Provider>
    );
};
