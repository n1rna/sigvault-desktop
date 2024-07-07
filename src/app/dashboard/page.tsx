"use client";

import { GlobeIcon } from "@radix-ui/react-icons";
import { useWalletConnection } from "@/hooks/wallet-connection";
export default function DashboardPage() {


  const { socketConnected, receivedMessages } =
    useWalletConnection();

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
      <footer className="w-full bg-gray-900 text-white px-6 py-4 flex items-center justify-between">
        {!socketConnected ? (
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
