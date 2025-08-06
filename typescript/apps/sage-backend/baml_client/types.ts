/**
 * Recursively partial type that can be null.
 *
 * @deprecated Use types from the `partial_types` namespace instead, which provides type-safe partial implementations
 * @template T The type to make recursively partial.
 */
export type RecursivePartialNull<T> = T extends object
  ? { [P in keyof T]?: RecursivePartialNull<T[P]> }
  : T | null;

export interface Checked<T, CheckName extends string = string> {
  value: T;
  checks: Record<CheckName, Check>;
}

export interface Check {
  name: string;
  expr: string;
  status: 'succeeded' | 'failed';
}

export function all_succeeded<CheckName extends string>(checks: Record<CheckName, Check>): boolean {
  return get_checks(checks).every((check) => check.status === 'succeeded');
}

export function get_checks<CheckName extends string>(checks: Record<CheckName, Check>): Check[] {
  return Object.values(checks);
}
export enum BamlLanguage {
  Baml = 'Baml',
  Python = 'Python',
  Typescript = 'Typescript',
  Javascript = 'Javascript',
  Ruby = 'Ruby',
  Go = 'Go',
  Other = 'Other',
}

export interface ContextDoc {
  title: string;
  body: string;
}

export interface Message {
  role: 'user' | 'assistant';
  text: string;
}

export interface Query {
  text: string;
  language_preference?: string | null;
  context_docs: ContextDoc[];
  prev_messages: Message[];
}

export interface QueryActionPlan {
  ranked_docs?: RelevantDoc[] | null;
  refine_query?: RefineQuery | null;
  answer?: string | null;
}

export interface RefineQuery {
  reason: string;
  suggested_queries: string[];
}

export interface RelevantDoc {
  title: string;
  relevance: 'very-relevant' | 'relevant' | 'not-relevant';
}

export interface Resume {
  name: string;
  email: string;
  experience: string[];
  skills: string[];
}

export interface SearchDocumentation {
  reason: string;
}
