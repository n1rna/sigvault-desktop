"use client";

import { Inter } from "next/font/google";
import { Providers } from "./providers";
import "./globals.css";
import 'react-toastify/dist/ReactToastify.css';

import { Cross1Icon, MinusIcon } from "@radix-ui/react-icons";
import { appWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/tauri'
import { useEffect } from "react";

const inter = Inter({ subsets: ["latin"] });


export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const handleMinimize = (e: React.MouseEvent) => {
    e.preventDefault();
    appWindow.minimize();
  }

  const handleClose = (e: React.MouseEvent) => {
    e.preventDefault();
    appWindow.close();
  }

  useEffect(() => {
    invoke('close_splashscreen').then((res) => {
      console.log('splashscreen closed')
    });
  }, [])

  return (
    <html lang="en">
      <div data-tauri-drag-region className="bg-stone-800 flex justify-end top-0 left-0 right-0 fixed select-none h-8">
        <div className="inline-flex justify-center items-center h-8 w-8 text-white hover:bg-stone-600" id="titlebar-minimize">
          <MinusIcon onClick={handleMinimize} />
        </div>
        <div className="inline-flex justify-center items-center h-8 w-8 text-white hover:bg-stone-600" id="titlebar-close">
          <Cross1Icon onClick={handleClose} />
        </div>
      </div>
      <body className={inter.className}>
        <Providers>
          <div className="flex flex-col items-center justify-center h-[384px] w-[512px] bg-gray-100 dark:bg-gray-950 shadow-lg mt-8">
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
