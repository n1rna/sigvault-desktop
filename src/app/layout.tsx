import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { Providers } from "./providers";
import "./globals.css";
import 'react-toastify/dist/ReactToastify.css';
// import { Avatar, AvatarFallback, AvatarImage } from "@/components/avatar";

const inter = Inter({ subsets: ["latin"] });


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
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
