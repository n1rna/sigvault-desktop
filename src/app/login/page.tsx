"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";

import { supabase } from "@/lib/supabase-client";
import { redirect } from "next/navigation";
import Auth from "@/components/auth";
import { ShadowNoneIcon } from "@radix-ui/react-icons";

export default function LoginPage() {
  const [session, setSession] = React.useState<AuthSession | null>(null);

  React.useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setSession(session);
    });

    supabase.auth.onAuthStateChange((_event, session) => {
      setSession(session);
    });
  }, []);



  if (session) {
    return redirect("/dashboard");
  }

  return (
    <>
      <div className="w-full flex items-center justify-between px-6 py-4 border-b">
        <div className="flex items-center">
          <ShadowNoneIcon className="w-4 h-4 text-primary" />
          <h1 className="ml-2 text-xl font-semibold">qblok</h1>
        </div>
      </div>
      <main className="flex-1 w-full pt-2">
        <Auth />
      </main>
    </>
  );
}
