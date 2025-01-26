import * as React from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardFooter,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Usb } from "lucide-react";
import { BackendCommandResult } from "@/lib/types";
import { useAppState } from "@/lib/providers";
import { Textarea } from "@/components/ui/textarea";

export default function TransactionSigningForm() {
  const [manualEntry, setManualEntry] = React.useState(false);
  const [signedPayload, setSignedPayload] = React.useState("");
  const [loadingSubmission, setLoadingSubmission] = React.useState(false);
  const fileInputRef = React.useRef<HTMLInputElement>(null);

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
            signed_payload: signedPayload,
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

  const handleDownloadPsbt = async () => {
    const tauri = await import("@tauri-apps/api/core");
    try {
      await tauri.invoke("cmd_download_psbt", {
        sessionId: current_session_id,
      });
    } catch (error) {
      console.error("Error downloading PSBT:", error);
    }
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (e) => {
        const content = e.target?.result as string;
        setSignedPayload(content);
      };
      reader.readAsText(file);
    }
  };

  const handleExit = async () => {
    await cleanupSession();
  };

  return (
    <Card className="w-full max-w-md mx-auto mt-8">
      <CardHeader>
        <CardTitle>Transaction Signing</CardTitle>
      </CardHeader>
      <CardContent>
        {!manualEntry ? (
          <div className="text-center">
            <Usb className="h-12 w-12 mx-auto text-primary" />
            <p className="mt-4 text-sm text-muted-foreground">
              Please connect your hardware wallet or sign the transaction
              manually.
            </p>
            <Button className="mt-4" onClick={() => setManualEntry(true)}>
              Sign Manually
            </Button>
          </div>
        ) : (
          <form onSubmit={handleSubmit}>
            <div className="grid w-full items-center gap-4">
              <div className="flex flex-col space-y-1.5">
                <Button onClick={handleDownloadPsbt}>Download PSBT</Button>
              </div>
              <div className="flex flex-col space-y-1.5">
                <Label htmlFor="signed-payload">Signed Transaction</Label>
                <Textarea
                  id="signed-payload"
                  placeholder="Paste signed transaction here"
                  value={signedPayload}
                  onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
                    setSignedPayload(e.target.value)
                  }
                  className="min-h-[100px]"
                />
              </div>
              <div className="flex flex-col space-y-1.5">
                <input
                  type="file"
                  ref={fileInputRef}
                  onChange={handleFileUpload}
                  className="hidden"
                  accept=".psbt"
                />
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => fileInputRef.current?.click()}
                >
                  Upload Signed PSBT
                </Button>
              </div>
            </div>
            <Button
              className="w-full mt-6"
              type="submit"
              disabled={loadingSubmission || !signedPayload}
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
