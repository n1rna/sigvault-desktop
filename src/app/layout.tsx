import type { Metadata } from "next";
import { Inter } from "next/font/google";
import "./globals.css";
import { Providers } from "./providers";

const inter = Inter({ subsets: ["latin"] });

export const metadata: Metadata = {
  title: "Qblok Vault",
  description: "Qblok Vault by 21st Capital",
};


export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={inter.className}>
        <Providers>
          <div className="flex flex-col items-center justify-center h-[500px] w-[700px] bg-gray-100 dark:bg-gray-950 shadow-lg ">
            <header className="w-full bg-gray-900 text-white px-6 py-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center">
                  <span className="text-lg font-bold not-selectable">qblok Vault</span>
                </div>
              </div>
            </header>
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
