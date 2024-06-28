"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";

import { supabase } from "@/lib/supabase-client";
import { redirect } from "next/navigation";

export default function Home() {
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

  return redirect("/login");
}
