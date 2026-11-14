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
    text: { content },
    type: 'text',
  }));
}

async function subscribeToNewsletter(email: string) {
  try {
    const response = await fetch(
      'https://app.loops.so/api/newsletter-form/cm3d7widq01su6gng1mc5iuvz',
      {
        body: new URLSearchParams({ email, mailingLists: '', userGroup: '' }),
        method: 'POST',
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
      message: 'Choose how you’d like to get started.',
      status: 'error',
    };

  const success = { signupType: signupType.id, status: 'success' } as const;
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
    return { message: 'Enter a valid email address.', status: 'error' };
  }
  const invalidTeammate = teammateEmails.find(
    (address) => !isSignupEmail(address),
  );
  if (invalidTeammate) {
    return {
      message: `Check the teammate email “${invalidTeammate.slice(0, 100)}”.`,
      status: 'error',
    };
  }
  if (teammateInput.length > SIGNUP_TEAMMATE_EMAILS_MAX) {
    return { message: 'The teammate email list is too long.', status: 'error' };
  }
  if (!name || !company || !stage) {
    return {
      message: 'Enter your name, company or team, and project stage.',
      status: 'error',
    };
  }
  if (
    name.length > SIGNUP_NAME_MAX ||
    company.length > SIGNUP_COMPANY_MAX ||
    project.length > SIGNUP_PROJECT_MAX
  ) {
    return {
      message: 'Shorten your name, company, or project description.',
      status: 'error',
    };
  }
  if (signupType.id === 'onboarding' && !eventId) {
    return { message: 'Choose a live onboarding session.', status: 'error' };
  }

  const token = process.env.NOTION_EAP_TOKEN;
  const dataSourceId = process.env.NOTION_EAP_DATA_SOURCE_ID;
  const unavailable = 'We couldn’t save your request. Please try again later.';
  if (!token || !dataSourceId) return { message: unavailable, status: 'error' };

  const earlier = previous.status === 'error' ? previous.retry : undefined;
  const retry =
    earlier?.email === email && earlier.eventId === eventId
      ? earlier
      : { email, eventId, id: randomUUID() };
  const notion = (path: string, method: 'POST' | 'PATCH', body: object) =>
    fetch(`https://api.notion.com/v1/${path}`, {
      body: JSON.stringify(body),
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
        'Notion-Version': '2026-03-11',
      },
      method,
      signal: AbortSignal.timeout(8000),
    });

  try {
    const session = eventId ? await getOnboardingSession(eventId) : null;
    if (eventId && !session) {
      return {
        message:
          'This session is unavailable. Choose another session or request a private session.',
        status: 'error',
      };
    }
    const registration = readRegistrationAnswers(
      session?.registrationQuestions ?? [],
      form,
      company,
    );
    if ('message' in registration)
      return { message: registration.message, retry, status: 'error' };

    const properties = {
      'Company or team': { rich_text: richText(company) },
      Email: { email },
      Name: { rich_text: richText(name) },
      'Project stage': { select: { name: stage[1] } },
      Request: { title: richText(company) },
      'Signup type': { select: { name: signupType.label } },
      'Use case': { rich_text: richText(project) },
      ...(signupType.id === 'access' && {
        'Teammate emails': { rich_text: richText(teammateEmails.join(', ')) },
      }),
      'Newsletter opt-in': { checkbox: true },
      'Submission ID': { rich_text: richText(retry.id) },
      'Tools in use': { rich_text: richText(tools.join(', ')) },
      ...(session && {
        'Luma answers': { rich_text: richText(registration.notes.join('\n')) },
        'Luma event ID': { rich_text: richText(session.id) },
        'Luma session': { url: session.url },
        'Session starts': { date: { start: session.start_at } },
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
          parent: { data_source_id: dataSourceId, type: 'data_source_id' },
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
          endAt: session.end_at,
          startAt: session.start_at,
          url: session.url,
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
        message: `We saved your details. ${result.error ?? 'Session registration is unavailable.'}`,
        retry,
        status: 'error',
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
    return { message: unavailable, retry, status: 'error' };
  }
}
