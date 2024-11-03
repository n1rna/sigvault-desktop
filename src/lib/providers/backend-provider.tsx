"use client";

import * as React from "react";
import { useBackendConnection } from "@/lib/hooks/use-backend-connection";

export type BackendContextType = {
    receivedMessages: { success: boolean; error?: string; message: { message_type: string; payload: any } }[];
    tryBackendConnection: () => void;
    instantiated: boolean;
    backendAuthenticated: boolean;
};

export const BackendContext = React.createContext<BackendContextType>({
    receivedMessages: [],
    tryBackendConnection: () => { },
    instantiated: false,
    backendAuthenticated: false,
});

interface BackendProviderProps {
    children: React.ReactNode;
}

export const BackendProvider = ({ children }: BackendProviderProps) => {
    const { receivedMessages, tryBackendConnection, instantiated, backendAuthenticated } = useBackendConnection();

    return (
        <BackendContext.Provider value={{
            receivedMessages, tryBackendConnection, instantiated, backendAuthenticated
        }}>
            {children}
        </BackendContext.Provider>
    );
};

