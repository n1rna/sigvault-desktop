"use client";

import Lottie from "lottie-react";
import circleGradient from "@/lib/lottie/circle-gradient-2.json"

import * as React from "react";

export default function DashboardPage() {

  return (
    <>
      <main className="flex-1 w-full p-6 flex flex-col items-center gap-10">
        <Lottie animationData={circleGradient} loop className="w-48" />
        <h2>Loading ...</h2>
      </main>
    </>
  );
}
