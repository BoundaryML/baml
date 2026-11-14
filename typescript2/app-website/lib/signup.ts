export const SIGNUP_TYPES = [
  {
    description:
      'Pick an upcoming session for a live overview of BAML and Boundary. See how code, tests, and traces fit together, ask questions, and get help setting up.',
    id: 'onboarding',
    label: 'Weekly live onboarding',
  },
  {
    description:
      'Bring your engineers to a private working session with ours. Walk through your architecture, data flows, and security or compliance requirements. We’ll work through what it would take to run Boundary in your environment.',
    heading: 'Private engineering session',
    id: 'access',
    label: 'Request a private session',
  },
] as const;

export const PROJECT_STAGES = [
  ['exploring', 'Exploring options'],
  ['prototype', 'Building a prototype'],
  ['production', 'Running in production'],
] as const;

export const SIGNUP_EMAIL_MAX = 254;

const JOIN_WINDOW_MS = 15 * 60 * 1000;

export function sessionTiming(
  session: { start_at: string; end_at: string },
  now: number,
) {
  const starts = Date.parse(session.start_at);
  const ends = Date.parse(session.end_at);
  if (now >= ends) return 'ended';
  if (now >= starts) return 'live';
  if (starts - now <= JOIN_WINDOW_MS) return 'soon';
  return 'upcoming';
}
export const SIGNUP_TEAMMATE_EMAILS_MAX = 2000;
export const SIGNUP_COMPANY_MAX = 120;
export const SIGNUP_PROJECT_MAX = 600;

export const SIGNUP_NAME_MAX = 120;
export const SIGNUP_TOOLS = [
  'OpenTelemetry',
  'Datadog',
  'Sentry',
  'Grafana',
  'LangSmith',
  'Langfuse',
  'Arize Phoenix',
  'Other',
] as const;
