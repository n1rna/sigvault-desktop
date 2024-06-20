"use client";

import { SuperTokensProvider } from "@/components/auth/supertokens-provider";

export function Providers({ children }: { children: React.ReactNode }) {
  return <SuperTokensProvider>{children}</SuperTokensProvider>;
}
