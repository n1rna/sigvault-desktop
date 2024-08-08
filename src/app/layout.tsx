"use client";

import { Inter } from "next/font/google";
import { Providers } from "./providers";
import "./globals.css";
import 'react-toastify/dist/ReactToastify.css';

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
      import('@tauri-apps/api').then((tauri) => {
        tauri.invoke("close_splashscreen").then(() => {
          setSplashscreenClosed(true)
        });
      });
    }, 2000);
  }, [splashscreenClosed])

  return (
    <html lang="en">
      <body className={inter.className}>
        <WindowTitlebar />
        <Providers>
          <div className="flex flex-col items-center justify-center h-[453px] w-[512px] bg-gray-100 dark:bg-gray-950 pt-8">
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
