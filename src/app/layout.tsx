"use client";

import { Inter } from "next/font/google";
import { Providers } from "./providers";
import "./globals.css";

import { WindowTitlebar } from "@/components/ui/window-titlebar";
import { useEffect, useState } from "react";
import { ShadowNoneIcon, PersonIcon } from "@radix-ui/react-icons";

const inter = Inter({ subsets: ["latin"] });

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const [splashscreenClosed, setSplashscreenClosed] = useState(false);
  useEffect(() => {
    if (splashscreenClosed) {
      return;
    }
    setTimeout(() => {
      import("@tauri-apps/api/core").then((tauri) => {
        tauri.invoke("cmd_close_splashscreen").then(() => {
          setSplashscreenClosed(true);
        });
      });
    }, 2000);
  }, [splashscreenClosed]);

  return (
    <html lang="en">
      <body className={inter.className}>
        <main className="flex flex-col flex-1 bg-gray-100 dark:bg-gray-950 h-[477px] w-[512px] overflow-hidden">
          <WindowTitlebar />
          <div className="min-h-8 w-full"></div>
          <Providers>
            <div className="overflow-auto">
              <div className="w-full flex flex-col flex-1">
                <div className="flex items-center justify-between px-6 py-4 border-b">
                  <div className="flex items-center">
                    <ShadowNoneIcon className="w-4 h-4 text-primary" />
                    <h1 className="ml-2 text-xl font-semibold">qblok</h1>
                  </div>
                  <div className="flex items-center gap-4">
                    <PersonIcon className="w-4 h-4" />
                  </div>
                </div>
                {children}
              </div>
            </div>
          </Providers>
        </main>
      </body>
    </html>
  );
}
