"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/components/supabase-client";
import { listen } from "@tauri-apps/api/event";


import { shell } from "@tauri-apps/api";
import { invoke } from "@tauri-apps/api";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { GitHubLogoIcon } from "@radix-ui/react-icons";



function getLocalHostUrl(port: number) {
  return `http://localhost:${port}`;
}

export default function Auth() {
  const [loading, setLoading] = useState(false);
  const [email, setEmail] = useState("");
  const [port, setPort] = useState<number | null>(null);

  useEffect(() => {
    console.log("Refresh", port);
    if (port) return;

    const unlisten = listen("oauth://url", (data) => {
      setPort(null);
      if (!data.payload) return;

      const url = new URL(data.payload as string);
      const code = new URLSearchParams(url.search).get("code");

      console.log("here", data.payload, code);
      if (code) {
        supabase.auth.exchangeCodeForSession(code).then(({ error }) => {
          if (error) {
            alert(error.message);
            console.error(error);
            return;
          }
          location.reload();
        });
      }
    });

    let _port: number | null = null;

    invoke("plugin:oauth|start").then(async (port) => {
      setPort(port as number);
      _port = port as number;
    });

    () => {
      unlisten?.then((u) => u());
      invoke("plugin:oauth|cancel", { port: _port });
    };
  }, [port]);

  const handleLogin = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setLoading(true);

    const { error } = await supabase.auth.signInWithOtp({
      email,
      options: { emailRedirectTo: getLocalHostUrl(port!) },
    });

    if (error) {
      alert(error.message);
    } else {
      alert("Check your email for the login link!");
    }
    setLoading(false);
  };

  const onProviderLogin = (provider: "google" | "github") => async () => {
    setLoading(true);

    const { data, error } = await supabase.auth.signInWithOAuth({
      options: {
        skipBrowserRedirect: true,
        scopes: provider === "google" ? "profile email" : "",
        redirectTo: getLocalHostUrl(port!),
      },
      provider: provider,
    });

    if (data.url) {
      shell.open(data.url);
    } else {
      alert(error?.message);
    }
  };

  return (
    <div className="max-w-md mx-auto">
      <h2 className="text-2xl font-bold mb-4">Login</h2>
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-4">
          <Button
            // onClick={onProviderLogin("github")}
            variant="outline"
            className="flex items-center justify-center col-span-2"
          >
            <GitHubLogoIcon className="h-5 w-5 mr-2" />
            Github
          </Button>
        </div>
        <div className="relative">
          <div className="absolute inset-0 flex items-center">
            <span className="w-full border-t" />
          </div>
          <div className="relative flex justify-center text-xs uppercase">
            <span className="bg-gray-100 px-2 text-gray-500 dark:bg-gray-950 dark:text-gray-400">
              Or continue with
            </span>
          </div>
        </div>
        <form className="space-y-4" onSubmit={handleLogin}>
          <div>
            <Label htmlFor="email">Email</Label>
            <Input
              id="email"
              type="email"
              placeholder="example@email.com"
              onChange={(e) => setEmail(e.target.value)}
              value={email}
              required={true}
            />
          </div>
          <Button type="submit" className="w-full">
            {loading ? <span>Loading</span> : <span>Sign in</span>}
          </Button>
        </form>
      </div>
    </div>
  );
}
