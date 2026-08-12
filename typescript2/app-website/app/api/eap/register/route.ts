import { NextResponse } from 'next/server';
import { type RegistrationAnswer, registerForEvent } from '@/lib/luma';

// Registers a guest for an EAP session directly against Luma so people don't
// have to leave the site. The Luma API key stays server-side here.
//
// NOTE: this endpoint is public. Before heavy promotion it should get basic
// abuse protection (rate limiting + a captcha such as Turnstile), since it can
// otherwise be scripted to create Luma guests.

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

interface RegisterBody {
  answers?: Record<string, string | boolean | string[]>;
  email?: string;
  eventId?: string;
  name?: string;
}

export async function POST(request: Request) {
  let body: RegisterBody;
  try {
    body = (await request.json()) as RegisterBody;
  } catch {
    return NextResponse.json({ error: 'Invalid request.' }, { status: 400 });
  }

  const eventId = body.eventId?.trim();
  const email = body.email?.trim();
  const name = body.name?.trim();

  if (!eventId || !eventId.startsWith('evt-')) {
    return NextResponse.json({ error: 'Missing event.' }, { status: 400 });
  }
  if (!email || !EMAIL_RE.test(email)) {
    return NextResponse.json(
      { error: 'Please enter a valid email address.' },
      { status: 400 },
    );
  }

  const answers: RegistrationAnswer[] = Object.entries(body.answers ?? {})
    .filter(([, value]) => value !== '' && value != null)
    .map(([question_id, value]) => ({ question_id, value }));

  const result = await registerForEvent({ answers, email, eventId, name });

  if (!result.ok) {
    return NextResponse.json({ error: result.error }, { status: 502 });
  }

  return NextResponse.json({ ok: true });
}
