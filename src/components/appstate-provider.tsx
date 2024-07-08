"use client";

import * as React from "react";

import { useWalletConnection } from "@/hooks/wallet-connection";

type AppStateContextType = {
};

export const AppStateContext = React.createContext<AppStateContextType>({
});

interface AppStateProviderProps {
    children: React.ReactNode;
}

export const SocketProvider = ({ children }: AppStateProviderProps) => {
    const { socketConnected, receivedMessages, retrySocketConnection } = useWalletConnection();

    return (
        <AppStateContext.Provider value={{ socketConnected, receivedMessages, retrySocketConnection }}>
            {children}
        </AppStateContext.Provider>
    );
};
