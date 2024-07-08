"use client";

import { SocketContext } from "@/components/socket-provider";
import Lottie from "lottie-react";
import circleGradient from "@/lib/lottie/circle-gradient-2.json"

import * as React from "react";

export default function DashboardPage() {

  const { socketConnected } =
    React.useContext(SocketContext);

  return (
    <>
      {socketConnected ? (

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
