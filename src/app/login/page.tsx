"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";
import { supabase } from "@/lib/supabase-client";
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
    <div className="container mx-auto p-4">
      <div className="flex items-center mb-4 gap-4">
        <h1 className="text-2xl font-bold">Login</h1>
      </div>
        <Auth />
    </div>
  );
}
