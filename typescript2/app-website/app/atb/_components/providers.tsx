"use client";

import { ConvexProvider, ConvexReactClient } from "convex/react";
import { useState } from "react";

export function Providers({ children }: { children: React.ReactNode }) {
  const [client] = useState(
    () => new ConvexReactClient(process.env.NEXT_PUBLIC_ATB_CONVEX_URL ?? ""),
  );
  return <ConvexProvider client={client}>{children}</ConvexProvider>;
}
