"use client";

import * as React from "react";
import { useAppState } from "@/lib/providers";
import { Loader2 } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import DeviceCreationForm from "./_components/DeviceCreationForm";
import TransactionSigningForm from "./_components/TransactionSigningForm";

export default function DashboardPage() {
  const {
    applicationState: {
      socket_connected,
      current_session_type,
      current_session_id,
      session_state,
    },
  } = useAppState();

  const renderSessionComponent = () => {
    console.log("current_session_type", current_session_type);
    switch (current_session_type) {
      case "DeviceCreation":
        return <DeviceCreationForm />;
      case "TransactionSigning":
        return <TransactionSigningForm />;
      default:
        return null;
    }
  };

  const renderLoading = () => (
    <Card className="w-full max-w-md mx-auto mt-8">
      <CardContent className="flex flex-col items-center justify-center p-6">
        <Loader2 className="h-8 w-8 animate-spin text-primary" />
        <p className="mt-4 text-center text-sm text-muted-foreground">
          Establishing connection to session {current_session_id}...
        </p>
      </CardContent>
    </Card>
  );
  return (
    <main className="flex-1 overflow-auto pb-6">
      {!socket_connected && <h2>Connection failed!</h2>}
      {socket_connected && !session_state && renderLoading()}
      {Boolean(socket_connected && session_state) && renderSessionComponent()}
    </main>
  );
}
