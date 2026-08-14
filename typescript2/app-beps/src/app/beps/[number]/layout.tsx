"use client";

import { ReactNode } from "react";
import { BepEditProvider } from "@/components/bep/bep-edit-context";
import { BepRouteShell } from "@/components/bep/bep-route-shell";

interface BepNumberLayoutProps {
  children: ReactNode;
}

export default function BepNumberLayout({ children }: BepNumberLayoutProps) {
  void children;
  return (
    <BepEditProvider>
      <BepRouteShell />
    </BepEditProvider>
  );
}
