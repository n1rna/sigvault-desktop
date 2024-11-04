"use client";

import * as React from "react";
import { ShadowNoneIcon, PersonIcon } from "@radix-ui/react-icons";

export default function DashboardPage({ children }: { children: React.ReactNode }) {

    return (
        <div className="w-full flex flex-col flex-1">
            <div className="flex items-center justify-between px-6 py-4 border-b">
                <div className="flex items-center">
                    <ShadowNoneIcon className="w-4 h-4 text-primary" />
                    <h1 className="ml-2 text-xl font-semibold">qblok</h1>
                </div>
                <div className="flex items-center gap-4">
                    <PersonIcon className="w-4 h-4" />
                </div>
            </div >
            {children}
        </div>
    );
}
