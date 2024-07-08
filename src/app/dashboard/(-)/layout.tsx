"use client";

import { GlobeIcon } from "@radix-ui/react-icons";
import { SocketContext } from "@/components/socket-provider";

import * as React from "react";
import { ShadowNoneIcon, LockClosedIcon, PersonIcon, LockOpen2Icon, ReloadIcon } from "@radix-ui/react-icons";

export default function DashboardPage({ children }: { children: React.ReactNode }) {

    const { socketConnected, retrySocketConnection } =
        React.useContext(SocketContext);

    const handleRetry = (e: React.MouseEvent<HTMLSpanElement>) => { e.preventDefault(); retrySocketConnection() } // Remove the second generic type argument from MouseEvent

    return (
        <>
            <header className="w-full flex items-center justify-between px-6 py-4 border-b">
                <div className="flex items-center">
                    <ShadowNoneIcon className="w-4 h-4 text-primary" />
                    <h1 className="ml-2 text-xl font-semibold">qblok</h1>
                </div>
                <div className="flex items-center gap-4">
                    {!socketConnected ? (
                        <div onClick={handleRetry} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <ReloadIcon className="w-4 h-4" />
                            <span>Retry Connection</span>
                        </div>
                    ) : (
                        <div onClick={handleRetry} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <LockClosedIcon className="w-4 h-4" />
                            <span>Connected</span>
                        </div>
                    )}
                    <PersonIcon className="w-4 h-4" />
                </div>
            </header >
            {children}
        </>
    );
}
