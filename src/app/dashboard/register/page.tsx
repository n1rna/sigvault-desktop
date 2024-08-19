/**
 * v0 by Vercel.
 * @see https://v0.dev/t/S3lESy22yJx
 * Documentation: https://v0.dev/docs#integrating-generated-code-into-your-nextjs-app
 */

"use client";

import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { useAppState } from "@/lib/providers";
import { useState } from "react";
import { useRouter } from "next/navigation";

import { BackendCommandResult } from "@/lib/types";

export default function Component() {
  const { actionPayload } = useAppState();

  const router = useRouter();

  const payload = actionPayload && actionPayload.payload;
  const [machineName, setMachineName] = useState("");

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    import('@tauri-apps/api').then((tauri) => {
      tauri.invoke<BackendCommandResult>("cmd_register_new_machine", { machineId: payload.machine_id, machineType: payload.machine_type, machineName }).then((resp) => {
        if (resp.success) {
          console.log("Machine registered successfully", resp);
        } else {
          console.error("Error registering machine", resp);
        }
      });
    });
  };


  const handleNameChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setMachineName(event.target.value);
  };

  return (
    <Card className="w-full max-w-md">
      <CardHeader>
        <CardTitle>Register New Machine</CardTitle>
      </CardHeader>
      <CardContent>
        <form className="space-y-4" onSubmit={handleSubmit}>
          <div className="space-y-2">
            <Label htmlFor="name">Machine Name</Label>
            <Input id="name" onChange={handleNameChange} value={machineName} placeholder="Enter machine name" />
          </div>
          <Button type="submit" className="w-full">
            Register
          </Button>
        </form>
        <div className="mt-6 border-t pt-6">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <p className="text-sm font-medium text-muted-foreground">Machine ID</p>
              {(payload) && (<p className="text-sm text-ellipsis overflow-hidden">{payload.machine_id}</p>)}
            </div>
            <div>
              <p className="text-sm font-medium text-muted-foreground">Machine Type</p>
              {(payload) && (<p className="text-sm text-ellipsis overflow-hidden">{payload.machine_type}</p>)}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}