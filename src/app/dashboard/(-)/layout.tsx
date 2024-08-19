"use client";

import * as React from "react";
import { ShadowNoneIcon, LockClosedIcon, PersonIcon, LockOpen2Icon, ReloadIcon } from "@radix-ui/react-icons";
import { useAppState } from "@/lib/providers";

export default function DashboardPage({ children }: { children: React.ReactNode }) {

    const { socket: { tryBackendConnection }, applicationState: { socket_connected } } = useAppState();

    const handleRetry = (e: React.MouseEvent<HTMLSpanElement>) => { e.preventDefault(); tryBackendConnection() } // Remove the second generic type argument from MouseEvent

    return (
        <>
            <div className="w-full flex items-center justify-between px-6 py-4 border-b">
                <div className="flex items-center">
                    <ShadowNoneIcon className="w-4 h-4 text-primary" />
                    <h1 className="ml-2 text-xl font-semibold">qblok</h1>
                </div>
                <div className="flex items-center gap-4">
                    {!socket_connected ? (
                        <button onClick={handleRetry} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <ReloadIcon className="w-4 h-4" />
                            <span>Retry Connection</span>
                        </button>
                    ) : (
                        <div onClick={handleRetry} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <LockClosedIcon className="w-4 h-4" />
                            <span>Connected</span>
                        </div>
                    )}
                    <PersonIcon className="w-4 h-4" />
                </div>
            </div >
            {children}
        </>
    );
}
