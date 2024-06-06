"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";

import { supabase } from "@/components/supabase-client";
import { redirect } from "next/navigation";
import Auth from "@/components/auth";

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
      <main className="flex-1 w-full p-6">
        <Auth />
      </main>
      <footer className="w-full bg-gray-900 text-white px-6 py-4">
        <p className="text-center text-sm">
          © 2024 21st Capital. All rights reserved.
        </p>
      </footer>
    </>
  );
}
