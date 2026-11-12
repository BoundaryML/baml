"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";

export function CopyToken({ token }: { token: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-center gap-2">
      <code className="flex-1 truncate rounded-md border px-3 py-2 text-xs">{token}</code>
      <Button
        size="sm"
        onClick={() => {
          void navigator.clipboard.writeText(token).then(() => setCopied(true));
        }}
      >
        {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}
