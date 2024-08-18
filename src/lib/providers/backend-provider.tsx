"use client";

import * as React from "react";
import { useBackendConnection } from "@/lib/hooks/use-backend-connection";

export type BackendContextType = {
    receivedMessages: { success: boolean; error?: string; message: { message_type: string; payload: any } }[];
    tryBackendConnection: () => void;
};

export const BackendContext = React.createContext<BackendContextType>({
    receivedMessages: [],
    tryBackendConnection: () => { },
});

interface BackendProviderProps {
    children: React.ReactNode;
}

export const BackendProvider = ({ children }: BackendProviderProps) => {
    const { receivedMessages, tryBackendConnection } = useBackendConnection();

    return (
        <BackendContext.Provider value={{
            receivedMessages, tryBackendConnection
        }}>
            {children}
        </BackendContext.Provider>
    );
};

