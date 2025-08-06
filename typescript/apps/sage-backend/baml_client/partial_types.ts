import type { RefineQuery, RelevantDoc } from './types';

/******************************************************************************
 *
 *  These types are used for streaming, for when an instance of a type
 *  is still being built up and any of its fields is not yet fully available.
 *
 ******************************************************************************/

export interface StreamState<T> {
  value: T;
  state: 'Pending' | 'Incomplete' | 'Complete';
}

export namespace partial_types {
  export interface ContextDoc {
    title?: string | null;
    body?: string | null;
  }
  export interface Message {
    role?: 'user' | 'assistant' | null;
    text?: string | null;
  }
  export interface Query {
    text?: string | null;
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
    reason?: string | null;
    suggested_queries: string[];
  }
  export interface RelevantDoc {
    title?: string | null;
    relevance?: 'very-relevant' | 'relevant' | 'not-relevant' | null;
  }
  export interface Resume {
    name?: string | null;
    email?: string | null;
    experience: string[];
    skills: string[];
  }
  export interface SearchDocumentation {
    reason?: string | null;
  }
}
