"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";

export function CopyToken({ token }: { token: string }) {
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <code className="flex-1 select-all break-all rounded-md border px-3 py-2 text-xs">
          {token}
        </code>
        <Button
          size="sm"
          onClick={() => {
            navigator.clipboard.writeText(token).then(
              () => setCopy("copied"),
              () => setCopy("failed")
            );
          }}
        >
          {copy === "copied" ? "Copied" : "Copy"}
        </Button>
      </div>
      {copy === "failed" && (
        <p role="alert" className="text-sm text-destructive">
          The browser did not allow copying. Click the token to select it, then copy it.
        </p>
      )}
    </div>
  );
}
