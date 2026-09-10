import type { ReactNode } from "react";
import { slackChannelUrl } from "@/lib/slack";

export function SlackLink({ children }: { children: ReactNode }) {
  const url = slackChannelUrl();
  return url ? <a className="underline" href={url}>{children}</a> : <span>{children}</span>;
}
