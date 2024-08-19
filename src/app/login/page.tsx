"use client";

import * as React from "react";

import { AuthSession } from "@supabase/supabase-js";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card"
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
    return redirect("/dashboard/home");
  }

  return (
    <Card className="w-full max-w-md">
      <CardHeader>
        <CardTitle>
          Login
        </CardTitle>
      </CardHeader>
      <CardContent>
        <Auth />
      </CardContent>
    </Card>

  );
}
