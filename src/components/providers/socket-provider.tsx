"use client";

import * as React from "react";
import { useWalletConnection } from "@/hooks/wallet-connection";

export type SocketContextType = {
    socketConnected: boolean;
    receivedMessages: { success: boolean; error?: string; message: { message_type: string; payload: any } }[];
    retrySocketConnection: () => void;
};

export const SocketContext = React.createContext<SocketContextType>({
    socketConnected: false,
    receivedMessages: [],
    retrySocketConnection: () => { },
});

interface SocketProviderProps {
    children: React.ReactNode;
}

export const SocketProvider = ({ children }: SocketProviderProps) => {
    const { socketConnected, receivedMessages, retrySocketConnection } = useWalletConnection();

    return (
        <SocketContext.Provider value={{
            socketConnected, receivedMessages, retrySocketConnection
        }}>
            {children}
        </SocketContext.Provider>
    );
};

