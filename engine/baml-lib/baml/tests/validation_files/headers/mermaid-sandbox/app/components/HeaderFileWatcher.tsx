"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef } from "react";

const WATCH_ENDPOINT = "/api/watch";

export default function HeaderFileWatcher() {
  const router = useRouter();
  const reconnectTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let isMounted = true;
    let eventSource: EventSource | null = null;

    const setup = () => {
      if (!isMounted) {
        return;
      }

      eventSource = new EventSource(WATCH_ENDPOINT);

      eventSource.onmessage = (event) => {
        if (event.data === "ready") {
          return;
        }
        router.refresh();
      };

      eventSource.onerror = () => {
        eventSource?.close();
        if (!isMounted) {
          return;
        }
        if (reconnectTimeout.current) {
          clearTimeout(reconnectTimeout.current);
        }
        reconnectTimeout.current = setTimeout(setup, 1000);
      };
    };

    setup();

    return () => {
      isMounted = false;
      eventSource?.close();
      if (reconnectTimeout.current) {
        clearTimeout(reconnectTimeout.current);
        reconnectTimeout.current = null;
      }
    };
  }, [router]);

  return null;
}
