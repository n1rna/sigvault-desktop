"use client";

import { SocketContext } from "@/components/socket-provider";

import * as React from "react";

export default function DashboardPage() {

    const { receivedMessages } =
        React.useContext(SocketContext);

    return (
        <>
            <main className="flex-1 w-full p-6 flex flex-col items-center justify-center">
                <div className="max-w-md mx-auto text-center">
                    {receivedMessages.map((msg, i) => (
                        <h5 className="text-md" key={i}>
                            {msg.success ? "Success" : "Error"}: {msg.error ? msg.error : `${msg.message.message_type}: ${JSON.stringify(msg.message.payload)}`}
                        </h5>
                    ))}
                </div>
            </main>
        </>
    );
}
