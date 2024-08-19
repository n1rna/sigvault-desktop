"use client";

import Lottie from "lottie-react";
import circleGradient from "@/lib/lottie/circle-gradient-2.json"

import * as React from "react";
import { useAppState } from "@/lib/providers";

export default function DashboardPage() {

  const { applicationState: { socket_connected }, socket: { tryBackendConnection, instantiated } } = useAppState();

  React.useEffect(() => {
    if (instantiated && !socket_connected) {
      tryBackendConnection();
    }
  }, [socket_connected, instantiated, tryBackendConnection]);

  return (
    <>
      {socket_connected ? (
        <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
          <Lottie animationData={circleGradient} loop className="w-48" />
          <h2>The application is waiting for commands from the server ...</h2>
        </main>
      ) : (
        <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
          <h2>You are disconnected from the server. Please try connecting again.</h2>
        </main>
      )}
    </>
  );
}
