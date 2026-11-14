'use client';

import { ArrowRight, CalendarDays, Check, Plus, Video } from 'lucide-react';
import { useActionState, useEffect, useRef, useState } from 'react';
import type { EapEvent } from '@/lib/luma';
import { dateLabel, daysUntil, timeLabel } from '@/lib/session-time';
import styles from './form-controls.module.css';
import SessionQuestion from './session-questions';
import TeammateEmails from './teammate-emails';
import { submitSignup } from '@/app/actions/signup';
import {
  PROJECT_STAGES,
  SIGNUP_COMPANY_MAX,
  SIGNUP_EMAIL_MAX,
  SIGNUP_PROJECT_MAX,
  SIGNUP_NAME_MAX,
  SIGNUP_TYPES,
  SIGNUP_TOOLS,
  sessionTiming,
} from '@/lib/signup';

const INPUT =
  'focus-ring mt-2 block w-full rounded-lg bg-raised px-3.5 py-3 text-head placeholder:text-muted';
const PRIMARY_ACTION =
  'focus-ring inline-flex min-h-12 w-full items-center justify-between gap-4 rounded-xl bg-accent px-5 py-3 text-left text-[15px] font-medium text-on-accent transition-colors hover:bg-accent-hover disabled:opacity-60 sm:w-auto';

type SignupSuccess = Extract<
  Awaited<ReturnType<typeof submitSignup>>,
  { status: 'success' }
>;

function SignupConfirmation({
  state,
  localTime,
  hostZone,
}: {
  state: SignupSuccess;
  localTime: { zone: string; now: Date } | undefined;
  hostZone: string | undefined;
}) {
  const card = useRef<HTMLDivElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus({ preventScroll: true });
    card.current?.scrollIntoView({ block: 'start' });
  }, []);

  const session = state.session;
  const timeZone = localTime?.zone ?? hostZone;
  const start = session ? new Date(session.startAt) : undefined;
  const now = localTime?.now.getTime();
  const ended =
    session && now !== undefined && now >= Date.parse(session.endAt);
  const live = start && now !== undefined && now >= start.getTime() && !ended;
  const zoomUrl = ended ? undefined : session?.zoomUrl;
  const onboarding = state.signupType === 'onboarding';

  return (
    <div
      className="ph-no-capture relative mx-auto max-w-[38rem] scroll-mt-28 rounded-3xl bg-raised/85 px-5 py-9 text-center shadow-2xl shadow-black/20 sm:px-14 sm:py-11"
      ref={card}
    >
      <div
        aria-hidden="true"
        className="relative mx-auto mb-4 grid size-28 place-items-center rounded-full bg-[radial-gradient(circle,color-mix(in_srgb,var(--accent)_18%,transparent),transparent_70%)]"
      >
        <Plus className="absolute top-4 right-2 size-3 text-accent" />
        <Plus className="absolute bottom-6 left-1 size-2.5 text-accent/70" />
        <span className="grid size-12 place-items-center rounded-full bg-accent text-on-accent">
          <Check size={24} strokeWidth={2.5} />
        </span>
      </div>
      <h1
        className="font-display text-4xl font-semibold tracking-tight text-head outline-none sm:text-5xl"
        id="request-access"
        ref={heading}
        tabIndex={-1}
      >
        {session
          ? 'You’re in.'
          : onboarding
            ? 'Registration submitted.'
            : 'Request received.'}
      </h1>
      <p className="mx-auto mt-3 max-w-[28rem] text-sm leading-relaxed text-ink-2 sm:text-base">
        {zoomUrl
          ? live
            ? 'The session is underway. Come on in.'
            : 'The session starts shortly. You can open Zoom now.'
          : session
            ? ended
              ? 'This session has ended.'
              : 'Your registration was sent to Luma. See the event page for details.'
            : onboarding
              ? 'Your registration was submitted.'
              : 'We’ll email you to find a time and discuss what your team would like to cover.'}
      </p>
      {session && (
        <div className="mt-8 flex flex-col items-center gap-4 border-t border-ink/10 pt-5 text-left sm:flex-row sm:justify-between">
          <div className="flex items-center gap-3">
            <CalendarDays
              aria-hidden="true"
              className="size-5 shrink-0 text-accent"
            />
            <div>
              <p className="text-sm font-medium text-head">
                Boundary live onboarding
              </p>
              {start && timeZone && (
                <time
                  className="mt-0.5 block text-sm text-ink-2"
                  dateTime={session.startAt}
                >
                  {dateLabel(start, timeZone)} · {timeLabel(start, timeZone)}
                </time>
              )}
            </div>
          </div>
          {zoomUrl ? (
            <a
              className={`${PRIMARY_ACTION} justify-center`}
              href={zoomUrl}
              referrerPolicy="no-referrer"
            >
              <Video aria-hidden="true" size={17} />
              Join Zoom
              <ArrowRight aria-hidden="true" size={16} />
            </a>
          ) : (
            <a
              className="focus-ring inline-flex items-center gap-2 rounded-sm text-sm font-medium text-accent underline-offset-4 hover:underline"
              href={session.url}
            >
              View on Luma <ArrowRight aria-hidden="true" size={15} />
            </a>
          )}
        </div>
      )}
      {state.newsletterFailed && (
        <p className="mt-4 text-xs text-ink-2">
          We couldn’t confirm your Boundary newsletter subscription.
        </p>
      )}
    </div>
  );
}

export default function SignupForm({
  sessions: availableSessions,
  previewZone,
  initialSessionId,
  signupAction = submitSignup,
}: {
  sessions: EapEvent[];
  previewZone?: string;
  initialSessionId?: string;
  signupAction?: typeof submitSignup;
}) {
  const [state, action, pending] = useActionState(signupAction, {
    status: 'idle',
  });
  const [signupType, setSignupType] = useState(
    initialSessionId ? 'onboarding' : 'access',
  );
  const [eventId, setEventId] = useState(initialSessionId ?? '');
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [company, setCompany] = useState('');
  const [teammateEmails, setTeammateEmails] = useState<string[]>([]);
  const [teammateDraft, setTeammateDraft] = useState('');
  const [stage, setStage] = useState('');
  const [project, setProject] = useState('');
  const [tools, setTools] = useState<string[]>([]);
  const [expanded, setExpanded] = useState(Boolean(initialSessionId));
  const [localTime, setLocalTime] = useState<{ zone: string; now: Date }>();
  useEffect(() => {
    const zone =
      previewZone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
    const updateTime = () => setLocalTime({ zone, now: new Date() });
    updateTime();
    const interval = window.setInterval(updateTime, 15_000);
    window.addEventListener('focus', updateTime);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener('focus', updateTime);
    };
  }, [previewZone]);
  const now = localTime?.now.getTime();
  const sessions = availableSessions.filter(
    (event) => now === undefined || Date.parse(event.end_at) > now,
  );
  const onboarding = signupType === 'onboarding';
  const session = sessions.find(({ id }) => id === eventId);
  const selectedTiming =
    session && now !== undefined ? sessionTiming(session, now) : undefined;

  if (state.status === 'success') {
    return (
      <SignupConfirmation
        hostZone={availableSessions.find(({ id }) => id === eventId)?.timezone}
        localTime={localTime}
        state={state}
      />
    );
  }

  return (
    <>
      <h1 className="v4-h1 max-w-[13ch] text-head" id="request-access">
        Get started with Boundary
      </h1>
      <p className="mt-5 max-w-[34rem] v4-body text-ink-2">
        Build a feedback loop with BAML in a live walkthrough, or bring your
        stack to a private session with our engineers.
      </p>
      <form
        action={action}
        aria-labelledby="request-access"
        className="ph-no-capture mt-9"
      >
        <fieldset className="min-w-0 space-y-9" disabled={pending}>
          <fieldset className="grid min-w-0 gap-x-8 gap-y-4 sm:grid-cols-2">
            <legend className="sr-only">
              How would you like to get started?
            </legend>
            {SIGNUP_TYPES.map((option) => (
              <label
                className="group relative cursor-pointer rounded-md py-2 pl-7 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-4 has-[:focus-visible]:outline-clover"
                key={option.id}
              >
                <input
                  checked={signupType === option.id}
                  className="sr-only"
                  name="signupType"
                  onChange={() => {
                    setSignupType(option.id);
                    setExpanded(true);
                  }}
                  type="radio"
                  value={option.id}
                />
                <span
                  aria-hidden="true"
                  className="absolute left-0 top-3 flex size-4 items-center justify-center rounded-full ring-1 ring-inset ring-white/35 text-transparent group-has-checked:bg-clover group-has-checked:text-ground group-has-checked:ring-0"
                >
                  <Check size={12} />
                </span>
                <span className="block text-lg font-medium tracking-tight text-head group-hover:text-clover group-has-checked:text-clover">
                  {'heading' in option ? option.heading : option.label}
                </span>
                <span className="mt-2 block text-sm leading-relaxed text-ink-2">
                  {option.description}
                </span>
              </label>
            ))}
          </fieldset>
          <div className="space-y-6">
            {onboarding && sessions.length > 0 && (
              <fieldset
                aria-describedby="session-time-zone"
                className="min-w-0"
              >
                <legend className="flex items-center gap-2 text-base font-medium text-head">
                  <CalendarDays
                    aria-hidden="true"
                    className="size-4 text-clover"
                  />
                  Choose a session
                </legend>
                <p className="mt-1 text-sm text-ink-2" id="session-time-zone">
                  {localTime
                    ? 'Dates and times are local to you.'
                    : 'Dates and times are in the host’s time zone.'}
                </p>
                <div className="mt-4 grid grid-cols-2 gap-2.5 sm:grid-cols-3 sm:gap-3">
                  {sessions.map((event) => {
                    const start = new Date(event.start_at);
                    const zone = localTime?.zone ?? event.timezone;
                    const day = dateLabel(start, zone);
                    const time = timeLabel(start, zone);
                    const hostDay = dateLabel(start, event.timezone);
                    const hostTime = timeLabel(start, event.timezone);
                    const days =
                      localTime && daysUntil(start, localTime.now, zone);
                    const timing =
                      now === undefined ? undefined : sessionTiming(event, now);
                    const when =
                      timing === 'live'
                        ? 'Happening now'
                        : timing === 'soon'
                          ? 'Starting soon'
                          : days === undefined || days < 0
                            ? null
                            : days === 0
                              ? 'Today'
                              : days === 1
                                ? 'Tomorrow'
                                : `In ${days} days`;
                    return (
                      <label
                        className="group flex min-w-0 cursor-pointer flex-col gap-1 rounded-xl bg-raised p-3.5 ring-1 ring-inset ring-transparent transition-colors hover:bg-clover/10 has-checked:bg-clover/12 has-checked:ring-clover/65 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-clover sm:p-4"
                        key={event.id}
                      >
                        <span className="flex items-center justify-between gap-2">
                          <span className="text-xs font-medium text-clover">
                            {timing === 'live' && (
                              <span
                                aria-hidden="true"
                                className="mr-1.5 inline-block size-1.5 rounded-full bg-current align-middle"
                              />
                            )}
                            {when}
                          </span>
                          <input
                            checked={eventId === event.id}
                            className={styles.choice}
                            name="eventId"
                            onChange={() => setEventId(event.id)}
                            required
                            type="radio"
                            value={event.id}
                          />
                        </span>
                        <span className="mt-1 text-base font-semibold tracking-tight text-head sm:text-xl">
                          {day}
                        </span>
                        <span className="text-sm text-head">{time}</span>
                        {(day !== hostDay || time !== hostTime) && (
                          <span className="mt-1 text-xs text-ink-2">
                            Host: {day !== hostDay ? `${hostDay}, ` : ''}
                            {hostTime}
                          </span>
                        )}
                        <span className="sr-only">{event.name}</span>
                      </label>
                    );
                  })}
                </div>
              </fieldset>
            )}
            {onboarding && sessions.length === 0 && (
              <div className="space-y-3 text-sm text-ink-2">
                <p role="status">No live sessions are available right now.</p>
                <button
                  className="focus-ring text-clover underline"
                  onClick={() => setSignupType('access')}
                  type="button"
                >
                  Request a private session
                </button>
              </div>
            )}
            {(!onboarding || sessions.length > 0) && (
              <div
                className="grid gap-4 sm:grid-cols-2"
                onFocus={() => setExpanded(true)}
              >
                <label className="block text-sm font-medium text-head">
                  Full name
                  <input
                    autoComplete="name"
                    className={INPUT}
                    maxLength={SIGNUP_NAME_MAX}
                    name="name"
                    onChange={(event) => setName(event.target.value)}
                    required
                    value={name}
                  />
                </label>
                <label className="block text-sm font-medium text-head">
                  Email
                  <input
                    autoComplete="email"
                    className={INPUT}
                    maxLength={SIGNUP_EMAIL_MAX}
                    name="email"
                    onChange={(event) => setEmail(event.target.value)}
                    required
                    type="email"
                    value={email}
                  />
                </label>
              </div>
            )}
            {expanded && (!onboarding || sessions.length > 0) && (
              <div className="space-y-6">
                <div className="grid gap-4 sm:grid-cols-2">
                  <label className="block text-sm font-medium text-head">
                    Company or team
                    <input
                      autoComplete="organization"
                      className={INPUT}
                      maxLength={SIGNUP_COMPANY_MAX}
                      name="company"
                      onChange={(event) => setCompany(event.target.value)}
                      required
                      placeholder="Independent if you’re working on your own"
                      value={company}
                    />
                  </label>
                  <label className="block text-sm font-medium text-head">
                    Where are you in your project?
                    <select
                      className={INPUT}
                      name="stage"
                      onChange={(event) => setStage(event.target.value)}
                      required
                      value={stage}
                    >
                      <option value="">Select a stage</option>
                      {PROJECT_STAGES.map(([value, label]) => (
                        <option key={value} value={value}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                {!onboarding && (
                  <TeammateEmails
                    draft={teammateDraft}
                    emails={teammateEmails}
                    inputClass={INPUT}
                    setDraft={setTeammateDraft}
                    setEmails={setTeammateEmails}
                  />
                )}
                <label className="block text-sm font-medium text-head">
                  What are you building?{' '}
                  <span className="font-normal text-muted">(optional)</span>
                  <textarea
                    className={INPUT}
                    maxLength={SIGNUP_PROJECT_MAX}
                    name="project"
                    onChange={(event) => setProject(event.target.value)}
                    placeholder={
                      onboarding
                        ? 'A sentence on your use case or what you’d like help with.'
                        : 'For example, tracing data retention, deployment in your environment, or early access.'
                    }
                    rows={3}
                    value={project}
                  />
                </label>
                <fieldset className="flex flex-wrap gap-x-6 gap-y-1">
                  <legend className="mb-2 text-sm font-medium text-head">
                    What tools do you already use?{' '}
                    <span className="font-normal text-muted">(optional)</span>
                  </legend>
                  {SIGNUP_TOOLS.map((tool) => (
                    <label
                      className="inline-flex min-h-8 cursor-pointer items-center gap-2 text-sm text-ink-2 hover:text-head has-checked:text-head"
                      key={tool}
                    >
                      <input
                        checked={tools.includes(tool)}
                        className={styles.choice}
                        name="tools"
                        onChange={(event) => {
                          const checked = event.target.checked;
                          setTools((current) =>
                            checked
                              ? [...current, tool]
                              : current.filter((item) => item !== tool),
                          );
                        }}
                        type="checkbox"
                        value={tool}
                      />
                      {tool}
                    </label>
                  ))}
                </fieldset>
                {onboarding &&
                  session &&
                  session.registrationQuestions.map((question) => (
                    <SessionQuestion
                      inputClass={INPUT}
                      key={`${session.id}:${question.id}`}
                      question={question}
                    />
                  ))}
                <div hidden>
                  <label>
                    Website
                    <input autoComplete="off" name="website" tabIndex={-1} />
                  </label>
                </div>
                {state.status === 'error' && (
                  <p className="text-sm text-red-300" role="alert">
                    {state.message}
                  </p>
                )}
                <button className={PRIMARY_ACTION} type="submit">
                  {pending
                    ? 'Submitting…'
                    : signupType === 'onboarding'
                      ? selectedTiming === 'live' || selectedTiming === 'soon'
                        ? 'Sign up to join'
                        : 'Sign up for live onboarding'
                      : 'Request a private session'}
                  <ArrowRight aria-hidden="true" size={17} />
                </button>
              </div>
            )}
          </div>
        </fieldset>
      </form>
    </>
  );
}
