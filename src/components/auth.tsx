"use client";

import { useEffect, useState } from "react";
import { supabase } from "@/lib/supabase-client";

import { listen } from "@tauri-apps/api/event";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { GitHubLogoIcon } from "@radix-ui/react-icons";

import { toast, Flip } from 'react-toastify';

import Lottie from "lottie-react";
import circleGradient from "@/lib/lottie/circle-gradient.json"


function getLocalHostUrl(port: number) {
  return `http://localhost:${port}`;
}

export default function Auth() {
  const [loading, setLoading] = useState(false);
  const [email, setEmail] = useState("");
  const [port, setPort] = useState<number | null>(null);

  const [waitingForAction, setWaitingForAction] = useState(false);

  useEffect(() => {
    if (port) return;

    const unlisten = listen("oauth://url", (data) => {
      setPort(null);
      if (!data.payload) return;

      const url = new URL(data.payload as string);
      const code = new URLSearchParams(url.search).get("code");

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

    import('@tauri-apps/api').then((tauri) => {
      tauri.invoke("plugin:oauth|start").then(async (port) => {
        setPort(port as number);
      });
    })

    return () => {
      unlisten?.then((u) => u());
      import('@tauri-apps/api').then((tauri) => {
        tauri.invoke("plugin:oauth|cancel", { port: port });
      })
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
      toast.error(error.message, {
        position: "top-right",
        autoClose: 5000,
        hideProgressBar: false,
        closeOnClick: true,
        pauseOnHover: true,
        draggable: true,
        progress: undefined,
        theme: "light",
        transition: Flip,
        bodyClassName: "text-sm",
      });
    } else {
      toast.info("Check your email for the login link.", {
        position: "top-right",
        autoClose: 5000,
        hideProgressBar: false,
        closeOnClick: true,
        pauseOnHover: true,
        draggable: true,
        progress: undefined,
        theme: "light",
        transition: Flip,
        bodyClassName: "text-sm",
      });
    }
    setLoading(false);
    setWaitingForAction(true);
  };

  return (
    <div className="max-w-md mx-auto">


      {waitingForAction ? (
        <div className="flex flex-col items-center gap-4 pt-6">
          <Lottie animationData={circleGradient} loop={true} className="w-32" />
          <p className="text-center text-lg">Please open the link sent to your email.</p>
          <h4 className="underline hover:cursor-pointer" onClick={() => {
            setWaitingForAction(false);
          }}>Try logging in again?</h4>
        </div>
      ) : (
        <>
          <h2 className="text-2xl font-bold mb-4">Login</h2>
          <div className="space-y-4 pt-6">
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
            </form >
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
          </div>
        </>
      )
      }
    </div >
  );
}
