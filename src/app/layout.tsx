"use client";

import { Inter } from "next/font/google";
import { Providers } from "./providers";
import "./globals.css";

import { WindowTitlebar } from "@/components/ui/window-titlebar";
import { useEffect, useState } from "react";

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
      import('@tauri-apps/api/core').then((tauri) => {
        tauri.invoke("cmd_close_splashscreen").then(() => {
          setSplashscreenClosed(true)
        });
      });
    }, 2000);
  }, [splashscreenClosed])

  return (
    <html lang="en">
      <body className={inter.className}>
        <main className="flex flex-col flex-1 h-[477px] w-[512px] overflow-hidden">
          <WindowTitlebar />
          <div className="min-h-8 w-full"></div>
          <Providers>
            <div className="bg-gray-100 dark:bg-gray-950 overflow-auto">
              {children}
            </div>
          </Providers>

        </main>
      </body>
    </html>
  );
}
