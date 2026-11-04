"use client";

import { useEffect, useState } from "react";

/** A ticking clock for relative times / heartbeat freshness (10s resolution). */
export function useNow(intervalMs = 10_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
