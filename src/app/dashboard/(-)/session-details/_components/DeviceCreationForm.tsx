// @/components/session/DeviceCreationForm.tsx
import * as React from "react";
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
import { Usb } from "lucide-react";
import type { BackendCommandResult } from "@/lib/types";
import { useAppState } from "@/lib/providers";

export default function DeviceCreationForm() {
  const [manualEntry, setManualEntry] = React.useState(false);
  const [xpub, setXpub] = React.useState("");
  const [derivationPath, setDerivationPath] = React.useState("");
  const [fingerprint, setFingerPrint] = React.useState("");
  const [loadingSubmission, setLoadingSubmission] = React.useState(false);

  const {
    applicationState: { current_session_id },
    cleanupSession,
  } = useAppState();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoadingSubmission(true);
    const tauri = await import("@tauri-apps/api/core");
    try {
      const resp = await tauri.invoke<BackendCommandResult>(
        "cmd_submituserinput_session_websocket",
        {
          input: JSON.stringify({
            xpub,
            derivation_path: derivationPath,
            fingerprint,
          }),
          sessionId: current_session_id,
        }
      );
      if (!resp.success) {
        console.error("Error submitting", resp);
      }
    } finally {
      setLoadingSubmission(false);
    }
  };

  const handleExit = async () => {
    const resp = await cleanupSession();
  };

  return (
    <Card className="w-full max-w-md mx-auto mt-8">
      <CardHeader>
        <CardTitle>Device Creation</CardTitle>
      </CardHeader>
      <CardContent>
        {!manualEntry ? (
          <div className="text-center">
            <Usb className="h-12 w-12 mx-auto text-primary" />
            <p className="mt-4 text-sm text-muted-foreground">
              Please connect your hardware wallet or enter the values manually.
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
            <Button
              className="w-full mt-6"
              type="submit"
              disabled={loadingSubmission}
            >
              {loadingSubmission ? "Submitting..." : "Submit"}
            </Button>
          </form>
        )}
      </CardContent>
      <CardFooter className="flex justify-between">
        {manualEntry ? (
          <Button variant="outline" onClick={() => setManualEntry(false)}>
            Back
          </Button>
        ) : (
          <Button variant="outline" onClick={handleExit}>
            Back
          </Button>
        )}
        <Button variant="ghost">Need Help?</Button>
      </CardFooter>
    </Card>
  );
}
