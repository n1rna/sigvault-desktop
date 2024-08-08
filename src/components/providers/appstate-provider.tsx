"use client";

import * as React from "react";

import { SocketContextType } from "@/components/providers/socket-provider";
import { useSocket } from "@/hooks";
import { useRouter } from "next/navigation";

type AppStateContextType = {
    socket: SocketContextType,
    actionCommand: string;
    actionPayload: any;
};

export const AppStateContext = React.createContext<AppStateContextType>({
    socket: {
        socketConnected: false,
        receivedMessages: [],
        retrySocketConnection: () => { },
    },
    actionCommand: "welcome",
    actionPayload: {}
});

interface AppStateProviderProps {
    children: React.ReactNode;
}

const SupportedMessageTypes = [
    "create_new_device",
    "device_created",
    "device_create_failed",
    "sign_transaction"
]

export const AppStateProvider = ({ children }: AppStateProviderProps) => {
    const socketState = useSocket();
    const [actionCommand, setActionCommand] = React.useState<string>("welcome");
    const [actionPayload, setActionPayload] = React.useState<any>({});

    const router = useRouter();

    React.useEffect(() => {
        if (!socketState.receivedMessages?.length) {
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

        if (lastMessage.message.message_type === "create_new_device") {
            router.push("/dashboard/devices");
        }
        else if (lastMessage.message.message_type === "device_created") {
            router.push("/dashboard");
        }

    }, [socketState.receivedMessages, router])

    return (
        <AppStateContext.Provider value={{
            socket: socketState,
            actionCommand,
            actionPayload
        }}>
            {children}
        </AppStateContext.Provider>
    );
};
