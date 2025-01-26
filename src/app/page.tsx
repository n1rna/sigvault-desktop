"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";

import { supabase } from "@/lib/supabase-client";
import { redirect } from "next/navigation";

export default function Home() {
  const [loading, setLoading] = React.useState(true);
  const [session, setSession] = React.useState<AuthSession | null>(null);

  React.useEffect(() => {
    supabase.auth.getSession().then(({ data: { session } }) => {
      setLoading(true);
      setSession(session);
      setLoading(false);
    });

    supabase.auth.onAuthStateChange((_event, session) => {
      setLoading(true);
      setSession(session);
      setLoading(false);
    });
  }, []);

  if (loading) {
    return <div>Loading...</div>;
  }

  if (session) {
    return redirect("/dashboard");
  }

  return redirect("/login");
}
