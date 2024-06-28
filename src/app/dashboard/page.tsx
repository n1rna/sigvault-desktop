"use client";

import { GlobeIcon } from "@radix-ui/react-icons";
import { useWalletConnection } from "@/hooks/wallet-connection";
export default function DashboardPage() {


  const { connection, waitingForConnection, initialized } =
    useWalletConnection();

  console.log("FFFFFFFFFFFFFFFFFFFFFFFFFFF", {
    connection,
    waitingForConnection,
    initialized,
  });

  return (
    <>
      <main className="flex-1 w-full p-6 flex flex-col items-center justify-center">
        <div className="max-w-md mx-auto text-center">
          {!waitingForConnection || !initialized ? (
            <h2 className="text-2xl font-bold mb-4">
              Waiting for connection
            </h2>
          ) : (
            <h2 className="text-2xl font-bold mb-4">
              Continue setting up your vault via the web app
            </h2>
          )}
        </div>
      </main>
      <footer className="w-full bg-gray-900 text-white px-6 py-4 flex items-center justify-between">
        {!waitingForConnection || !initialized ? (
          <>
            <div className="flex items-center">
              <GlobeIcon className="h-6 w-6 mr-2" />
              <span className="text-sm">
                Application is trying to connect to server
              </span>
            </div>
            <div className="flex items-center">
              <span className="h-3 w-3 rounded-full bg-yellow-500 mr-2" />
              <span className="text-sm">Connecting...</span>
            </div>
          </>
        ) : (
          <>
            <div className="flex items-center">
              <GlobeIcon className="h-6 w-6 mr-2" />
              <span className="text-sm">Application is now connected</span>
            </div>
            <div className="flex items-center">
              <span className="h-3 w-3 rounded-full bg-green-500 mr-2" />
              <span className="text-sm">Connected</span>
            </div>
          </>
        )}
      </footer>
    </>
  );
}
