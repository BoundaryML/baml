import { type NextRequest, NextResponse } from 'next/server';
import { currentUser, issuePath, siteOrigin } from '@/lib/approval-auth';
import {
  type ApprovalIssue,
  isShepherd,
  issueStore,
} from '@/lib/issue-approval';
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> },
) {
  try {
    if (request.headers.get('origin') !== siteOrigin())
      return new NextResponse('Forbidden', { status: 403 });
    const login = await currentUser();
    if (!login) return new NextResponse('Sign in first', { status: 403 });
    const { id } = await params;
    let path: string;
    try {
      path = issuePath(id);
    } catch {
      return new NextResponse('Invalid issue', { status: 400 });
    }
    const filter = `id=eq.${encodeURIComponent(id)}&dataset=eq.live`;
    const [issue] = await issueStore<ApprovalIssue[]>(
      `select=id,shepherd,status&${filter}&limit=1`,
    );
    if (!issue || !isShepherd(login, issue))
      return new NextResponse('Assigned shepherd required', { status: 403 });
    const rows = await issueStore<ApprovalIssue[]>(
      `${filter}&state=eq.awaiting_approval&shepherd=eq.${encodeURIComponent(issue.shepherd!)}`,
      {
        body: JSON.stringify({
          status: { by: `github:${login}`, state: 'approved' },
        }),
        method: 'PATCH',
      },
    );
    if (rows.length !== 1)
      return new NextResponse(
        'Issue changed or was already approved. Refresh the page.',
        { status: 409 },
      );
    return NextResponse.redirect(new URL(path, siteOrigin()), 303);
  } catch {
    return new NextResponse(
      'Approval unavailable. Please retry or use Slack.',
      { status: 503 },
    );
  }
}
