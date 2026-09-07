import { NextRequest, NextResponse } from "next/server";
import { currentApprover, proposalPath, siteOrigin } from "@/lib/approval-auth";
import { proposalStore, type Proposal } from "@/lib/proposals";

export async function POST(request: NextRequest, { params }: { params: Promise<{ id: string }> }) {
  try {
    // Explicit Origin validation protects cookie-authenticated writes from CSRF.
    if (request.headers.get("origin") !== siteOrigin()) return new NextResponse("Forbidden", { status: 403 });
    const login = await currentApprover();
    if (!login) return new NextResponse("Maintainer sign-in required", { status: 403 });
    const { id } = await params;
    const path = proposalPath(id);
    const form = await request.formData();
    const head = form.get("head");
    if (typeof head !== "string" || !/^[a-f0-9]{40}$/.test(head)) return new NextResponse("Invalid head", { status: 400 });
    const saved = await proposalStore<Proposal[]>(`id=eq.${encodeURIComponent(id)}&head=eq.${head}&status=eq.pending&dataset=eq.live`, {
      method: "PATCH", body: JSON.stringify({ status: "approved", approved_by: `github:${login}`, approved_at: new Date().toISOString() }),
    });
    if (saved.length !== 1) return new NextResponse("Proposal already handled or changed. Refresh before approving.", { status: 409 });
    return NextResponse.redirect(new URL(path, siteOrigin()), 303);
  } catch { return new NextResponse("Approval could not be saved. Please retry.", { status: 503 }); }
}
