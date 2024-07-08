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
      <header className="w-full flex items-center justify-between px-6 py-4 border-b">
        <div className="flex items-center">
          <ShadowNoneIcon className="w-4 h-4 text-primary" />
          <h1 className="ml-2 text-xl font-semibold">qblok</h1>
        </div>
      </header>
      <main className="flex-1 w-full p-6">
        <Auth />
      </main>
      <footer className="w-full px-6 py-4">
        <p className="text-center text-sm">
          © 2024 qblok
        </p>
      </footer>
    </>
  );
}
