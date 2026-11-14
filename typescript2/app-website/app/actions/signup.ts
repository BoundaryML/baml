'use server';

import { randomUUID } from 'node:crypto';
import {
  getOnboardingSession,
  registerForEvent,
  zoomAttendeeUrl,
} from '@/lib/luma';
import { readRegistrationAnswers } from '@/lib/luma-answers';
import {
  PROJECT_STAGES,
  SIGNUP_COMPANY_MAX,
  SIGNUP_NAME_MAX,
  SIGNUP_PROJECT_MAX,
  SIGNUP_TEAMMATE_EMAILS_MAX,
  SIGNUP_TOOLS,
  SIGNUP_TYPES,
  sessionTiming,
} from '@/lib/signup';
import { isSignupEmail, parseTeammateEmails } from '@/lib/signup-email';

type SignupState =
  | { status: 'idle' }
  | {
      status: 'success';
      signupType: (typeof SIGNUP_TYPES)[number]['id'];
      newsletterFailed?: boolean;
      session?: {
        url: string;
        startAt: string;
        endAt: string;
        zoomUrl?: string;
      };
    }
  | {
      status: 'error';
      message: string;
      retry?: { id: string; email: string; eventId: string };
    };

function richText(value: string) {
  return (value.match(/[\s\S]{1,1000}/gu) ?? []).map((content) => ({
    type: 'text',
    text: { content },
  }));
}

async function subscribeToNewsletter(email: string) {
  try {
    const response = await fetch(
      'https://app.loops.so/api/newsletter-form/cm3d7widq01su6gng1mc5iuvz',
      {
        method: 'POST',
        body: new URLSearchParams({ email, userGroup: '', mailingLists: '' }),
        signal: AbortSignal.timeout(5000),
      },
    );
    if (response.ok) return true;
    console.error('Newsletter signup failed:', response.status);
  } catch {
    console.error('Newsletter signup could not be completed.');
  }
  return false;
}

export async function submitSignup(
  previous: SignupState,
  form: FormData,
): Promise<SignupState> {
  const signupType = SIGNUP_TYPES.find(
    ({ id }) => id === form.get('signupType'),
  );
  if (!signupType)
    return {
      status: 'error',
      message: 'Choose how you’d like to get started.',
    };

  const success = { status: 'success', signupType: signupType.id } as const;
  if (form.get('website')) return success;

  const email = String(form.get('email') ?? '')
    .trim()
    .toLowerCase();
  const name = String(form.get('name') ?? '').trim();
  const company = String(form.get('company') ?? '').trim();
  const project = String(form.get('project') ?? '').trim();
  const toolValues = form.getAll('tools');
  const tools = SIGNUP_TOOLS.filter((tool) => toolValues.includes(tool));
  const stage = PROJECT_STAGES.find(([value]) => value === form.get('stage'));
  const eventId =
    signupType.id === 'onboarding' ? String(form.get('eventId') ?? '') : '';

  const teammateInput =
    signupType.id === 'access' ? form.getAll('teammateEmails').join(', ') : '';
  const teammateEmails = parseTeammateEmails(teammateInput).filter(
    (address) => address !== email,
  );

  if (!isSignupEmail(email)) {
    return { status: 'error', message: 'Enter a valid email address.' };
  }
  const invalidTeammate = teammateEmails.find(
    (address) => !isSignupEmail(address),
  );
  if (invalidTeammate) {
    return {
      status: 'error',
      message: `Check the teammate email “${invalidTeammate.slice(0, 100)}”.`,
    };
  }
  if (teammateInput.length > SIGNUP_TEAMMATE_EMAILS_MAX) {
    return { status: 'error', message: 'The teammate email list is too long.' };
  }
  if (!name || !company || !stage) {
    return {
      status: 'error',
      message: 'Enter your name, company or team, and project stage.',
    };
  }
  if (
    name.length > SIGNUP_NAME_MAX ||
    company.length > SIGNUP_COMPANY_MAX ||
    project.length > SIGNUP_PROJECT_MAX
  ) {
    return {
      status: 'error',
      message: 'Shorten your name, company, or project description.',
    };
  }
  if (signupType.id === 'onboarding' && !eventId) {
    return { status: 'error', message: 'Choose a live onboarding session.' };
  }

  const token = process.env.NOTION_EAP_TOKEN;
  const dataSourceId = process.env.NOTION_EAP_DATA_SOURCE_ID;
  const unavailable = 'We couldn’t save your request. Please try again later.';
  if (!token || !dataSourceId) return { status: 'error', message: unavailable };

  const earlier = previous.status === 'error' ? previous.retry : undefined;
  const retry =
    earlier?.email === email && earlier.eventId === eventId
      ? earlier
      : { id: randomUUID(), email, eventId };
  const notion = (path: string, method: 'POST' | 'PATCH', body: object) =>
    fetch(`https://api.notion.com/v1/${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
        'Notion-Version': '2026-03-11',
      },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(8000),
    });

  try {
    const session = eventId ? await getOnboardingSession(eventId) : null;
    if (eventId && !session) {
      return {
        status: 'error',
        message:
          'This session is unavailable. Choose another session or request a private session.',
      };
    }
    const registration = readRegistrationAnswers(
      session?.registrationQuestions ?? [],
      form,
      company,
    );
    if ('message' in registration)
      return { status: 'error', message: registration.message, retry };

    const properties = {
      Request: { title: richText(company) },
      Name: { rich_text: richText(name) },
      Email: { email },
      'Company or team': { rich_text: richText(company) },
      'Signup type': { select: { name: signupType.label } },
      'Project stage': { select: { name: stage[1] } },
      'Use case': { rich_text: richText(project) },
      ...(signupType.id === 'access' && {
        'Teammate emails': { rich_text: richText(teammateEmails.join(', ')) },
      }),
      'Tools in use': { rich_text: richText(tools.join(', ')) },
      'Newsletter opt-in': { checkbox: true },
      'Submission ID': { rich_text: richText(retry.id) },
      ...(session && {
        'Luma event ID': { rich_text: richText(session.id) },
        'Luma session': { url: session.url },
        'Session starts': { date: { start: session.start_at } },
        'Luma answers': { rich_text: richText(registration.notes.join('\n')) },
      }),
    };

    let saved:
      | {
          id: string;
          properties?: { 'Registration submitted'?: { checkbox: boolean } };
        }
      | undefined;
    if (earlier === retry) {
      const lookup = await notion(
        `data_sources/${dataSourceId}/query`,
        'POST',
        {
          filter: {
            property: 'Submission ID',
            rich_text: { equals: retry.id },
          },
          page_size: 1,
        },
      );
      if (!lookup.ok)
        throw new Error(`Notion lookup failed (${lookup.status})`);
      saved = (await lookup.json()).results[0];
    }
    const response = saved
      ? await notion(`pages/${saved.id}`, 'PATCH', { properties })
      : await notion('pages', 'POST', {
          parent: { type: 'data_source_id', data_source_id: dataSourceId },
          properties: {
            ...properties,
            'Follow-up': { select: { name: 'New' } },
          },
        });
    if (!response.ok)
      throw new Error(`Notion write failed (${response.status})`);
    async function confirmSignup(): Promise<SignupState> {
      const newsletterFailed = !(await subscribeToNewsletter(email));
      if (!session) return { ...success, newsletterFailed };
      const timing = sessionTiming(session, Date.now());
      return {
        ...success,
        newsletterFailed,
        session: {
          url: session.url,
          startAt: session.start_at,
          endAt: session.end_at,
          zoomUrl:
            timing === 'soon' || timing === 'live'
              ? zoomAttendeeUrl(session.meeting_url)
              : undefined,
        },
      };
    }
    if (!session || saved?.properties?.['Registration submitted']?.checkbox)
      return confirmSignup();

    const page = saved ?? ((await response.json()) as { id: string });
    const result = await registerForEvent({
      answers: registration.answers,
      email,
      eventId: session.id,
      name,
      sendEmail: false,
    });
    if (!result.ok) {
      return {
        status: 'error',
        message: `We saved your details. ${result.error ?? 'Session registration is unavailable.'}`,
        retry,
      };
    }

    const [confirmation] = await Promise.all([
      confirmSignup(),
      notion(`pages/${page.id}`, 'PATCH', {
        properties: { 'Registration submitted': { checkbox: true } },
      })
        .then((marker) => {
          if (!marker.ok)
            console.error(
              'Notion registration status update failed:',
              marker.status,
            );
        })
        .catch(() => {
          console.error('Notion registration status could not be updated.');
        }),
    ]);
    return confirmation;
  } catch {
    console.error('Early access request could not be completed.');
    return { status: 'error', message: unavailable, retry };
  }
}
