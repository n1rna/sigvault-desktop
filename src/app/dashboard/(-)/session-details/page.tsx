"use client";

import * as React from "react";
import { useAppState } from "@/lib/providers";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Loader2, Usb } from "lucide-react";
import { BackendCommandResult } from "@/lib/types";

export default function DashboardPage() {
  const [manualEntry, setManualEntry] = React.useState(false);
  const [xpub, setXpub] = React.useState("");
  const [derivationPath, setDerivationPath] = React.useState("");
  const [fingerprint, setFingerPrint] = React.useState("");

  const [loadingSubmission, setLoadingSubmission] = React.useState(false);

  const {
    applicationState: {
      socket_connected,
      current_session_type,
      current_session_id,
      session_state,
    },
  } = useAppState();

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    console.log("Submitted:", { xpub, derivationPath, fingerprint });
    setLoadingSubmission(true);

    import("@tauri-apps/api/core").then((tauri) => {
      tauri
        .invoke<BackendCommandResult>("cmd_submituserinput_session_websocket", {
          input: JSON.stringify({ xpub, derivation_path: derivationPath, fingerprint }),
          sessionId: current_session_id,
        })
        .then((resp) => {
          if (resp.success) {
            console.log("device creation submitted successfully", resp);
          } else {
            console.error("Error submitting", resp);
          }
          setLoadingSubmission(false);
        });
    });
  };

  const loading = () => {
    return (
      <Card className="w-full max-w-md mx-auto mt-8">
        <CardContent className="flex flex-col items-center justify-center p-6">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <p className="mt-4 text-center text-sm text-muted-foreground">
            Establishing connection to session {current_session_id}...
          </p>
        </CardContent>
      </Card>
    );
  };

  return (
    <main className="flex-1 overflow-auto pb-6">
      {!socket_connected && <h2>Connection failed!</h2>}
      {socket_connected && !session_state && loading()}
      {Boolean(socket_connected && session_state) && (
        <Card className="w-full max-w-md mx-auto mt-8">
          <CardHeader>
            <CardTitle>Device Creation</CardTitle>
          </CardHeader>
          <CardContent>
            {!manualEntry ? (
              <div className="text-center">
                <Usb className="h-12 w-12 mx-auto text-primary" />
                <p className="mt-4 text-sm text-muted-foreground">
                  Please connect your hardware wallet or enter the values
                  manually.
                </p>
                <Button className="mt-4" onClick={() => setManualEntry(true)}>
                  Enter Manually
                </Button>
              </div>
            ) : (
              <form onSubmit={handleSubmit}>
                <div className="grid w-full items-center gap-4">
                  <div className="flex flex-col space-y-1.5">
                    <Label htmlFor="xpub">Extended Public Key (xpub)</Label>
                    <Input
                      id="xpub"
                      placeholder="Enter xpub"
                      value={xpub}
                      onChange={(e) => setXpub(e.target.value)}
                    />
                  </div>
                  <div className="flex flex-col space-y-1.5">
                    <Label htmlFor="derivation-path">Derivation Path</Label>
                    <Input
                      id="derivation-path"
                      placeholder="e.g., m/44'/0'/0'"
                      value={derivationPath}
                      onChange={(e) => setDerivationPath(e.target.value)}
                    />
                  </div>
                  <div className="flex flex-col space-y-1.5">
                    <Label htmlFor="fingerprint">Finger Print</Label>
                    <Input
                      id="fingerprint"
                      placeholder="Enter fingerprint"
                      value={fingerprint}
                      onChange={(e) => setFingerPrint(e.target.value)}
                    />
                  </div>
                </div>
                {loadingSubmission ? (
                  <p className="mt-4 text-center text-sm text-muted-foreground">
                    Submitting...
                  </p>
                ) : (
                  <Button className="w-full mt-6" type="submit">
                    Submit
                  </Button>
                )}
              </form>
            )}
          </CardContent>
          <CardFooter className="flex justify-between">
            <Button variant="outline" onClick={() => setManualEntry(false)}>
              Back
            </Button>
            <Button variant="ghost" onClick={() => console.log("Help clicked")}>
              Need Help?
            </Button>
          </CardFooter>
        </Card>
      )}
    </main>
  );
}
