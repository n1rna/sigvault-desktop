"use client";

import Lottie from "lottie-react";
import circleGradient from "@/lib/lottie/circle-gradient-2.json"

import * as React from "react";
import { useAppState } from "@/lib/providers";

export default function DashboardPage() {

  const { socket: { tryBackendConnection, backendAuthenticated } } = useAppState();

  return (
    <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
      {!backendAuthenticated && (
        <>
          <Lottie animationData={circleGradient} loop className="w-48" />
          <h2>Application is disconnected from the server ...</h2>
          <button
            onClick={tryBackendConnection}
            className="px-4 py-2 bg-blue-500 text-white rounded-md hover:bg-blue-600 transition-colors"
          >
            Retry Connection
          </button>
        </>
      )}
      {backendAuthenticated && (
        <h2>Loading ...</h2>
      )}
    </main>
  );
}
