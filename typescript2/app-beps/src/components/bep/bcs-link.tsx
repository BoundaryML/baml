"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";

/**
 * The link to the team-only BCS proposals, shown when the server says the viewer may read them.
 * It only hides the link: the pages themselves check the viewer on the server.
 */
export function BcsLink() {
  const [member, setMember] = useState(false);
  useEffect(() => {
    fetch("/api/bcs/session", { cache: "no-store" })
      .then((response) => response.json())
      .then((session: { member?: boolean }) => setMember(session.member === true))
      .catch(() => setMember(false));
  }, []);
  if (!member) return null;
  return (
    <Link href="/bcs">
      <Button variant="outline" size="sm">
        BCS proposals
      </Button>
    </Link>
  );
}
