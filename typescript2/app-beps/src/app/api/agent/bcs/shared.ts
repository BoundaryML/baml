// What the BCS agent routes share: the CLI token check, the Convex client and JSON answers.
import { NextRequest, NextResponse } from "next/server";
import { type BcsViewer, verifyCliRequest } from "@/lib/bcs-auth";
import { BCS_STATUSES, type BcsFailure, type BcsStatus } from "@/lib/bcs-convex";

export function json(body: unknown, status = 200): NextResponse {
  return NextResponse.json(body, { status, headers: { "Cache-Control": "no-store" } });
}

/** The member the request's CLI token names, or the 401 to answer. */
export function authenticate(request: NextRequest): BcsViewer | NextResponse {
  return (
    verifyCliRequest(request.headers) ??
    json({ error: "A BCS token is required: get yours at /bcs/token." }, 401)
  );
}

/** A status from a request: undefined when omitted, an error when it is not one we know. */
export function status(value: unknown): BcsStatus | undefined | { error: string } {
  if (value === undefined || value === null) return undefined;
  return (
    BCS_STATUSES.find((each) => each === value) ?? {
      error: `status must be one of ${BCS_STATUSES.join(", ")}`,
    }
  );
}

const FAILURE_STATUS: Record<BcsFailure["kind"], number> = {
  not_configured: 503,
  stale: 409,
  missing: 404,
  invalid: 400,
};

/** A refused BCS call, answered with its reason and the matching HTTP status. */
export function refused(failure: BcsFailure): NextResponse {
  return json({ error: failure.message }, FAILURE_STATUS[failure.kind]);
}
