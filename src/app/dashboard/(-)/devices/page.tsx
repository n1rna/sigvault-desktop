"use client";

import { useAppState } from "@/hooks";

import * as React from "react";

export default function DashboardPage() {

    const { actionCommand, actionPayload } = useAppState();

    return (
        <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
            <h2>{actionCommand} - {JSON.stringify(actionPayload)}</h2>
        </main>
    );
}
