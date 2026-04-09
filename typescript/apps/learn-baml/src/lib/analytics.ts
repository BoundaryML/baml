import posthog from 'posthog-js';

declare global {
  interface Window {
    __POSTHOG_INITIALIZED__?: boolean;
  }
}

const POSTHOG_KEY = typeof window !== 'undefined'
  ? (window as { NEXT_PUBLIC_POSTHOG_KEY?: string }).NEXT_PUBLIC_POSTHOG_KEY
  : undefined;
const POSTHOG_HOST = 'https://app.posthog.com';

export function initAnalytics() {
  if (typeof window === 'undefined' || window.__POSTHOG_INITIALIZED__ || !POSTHOG_KEY) return;

  posthog.init(POSTHOG_KEY, {
    api_host: POSTHOG_HOST,
    capture_pageview: false, // Docusaurus handles this
    capture_pageleave: true,
    persistence: 'localStorage',
  });

  window.__POSTHOG_INITIALIZED__ = true;
}

// ============ Ask BAML Events ============

export function trackAssistantOpened(source: 'button' | 'keyboard') {
  if (typeof window === 'undefined') return;
  posthog.capture('ask_baml_opened', {
    source,
    page_url: window.location.pathname,
    page_title: document.title,
  });
}

export function trackQuery(params: {
  query: string;
  sessionId: string;
  conversationLength: number;
}) {
  if (typeof window === 'undefined') return;
  posthog.capture('ask_baml_query', {
    query: params.query,
    session_id: params.sessionId,
    conversation_length: params.conversationLength,
    page_url: window.location.pathname,
  });
}

export function trackResponse(params: {
  query: string;
  answer: string;
  citations: Array<{ title: string; url: string; relevance: string }>;
  suggestedQuestions: string[];
  latencyMs: number;
  topDocScores: number[];
}) {
  posthog.capture('ask_baml_response', {
    query: params.query,
    answer_preview: params.answer.slice(0, 500), // First 500 chars
    answer_length: params.answer.length,
    citation_count: params.citations.length,
    citations: params.citations,
    suggestion_count: params.suggestedQuestions.length,
    latency_ms: params.latencyMs,
    top_doc_scores: params.topDocScores,
  });
}

export function trackCitationClick(params: {
  citationUrl: string;
  citationTitle: string;
  query: string;
}) {
  posthog.capture('ask_baml_citation_clicked', {
    citation_url: params.citationUrl,
    citation_title: params.citationTitle,
    query: params.query,
  });
}

export function trackSuggestionClick(params: {
  suggestion: string;
  originalQuery: string;
}) {
  posthog.capture('ask_baml_suggestion_clicked', {
    suggestion: params.suggestion,
    original_query: params.originalQuery,
  });
}

export function trackFeedback(params: {
  query: string;
  answer: string;
  rating: 'positive' | 'negative';
  feedbackText?: string;
}) {
  posthog.capture('ask_baml_feedback', {
    query: params.query,
    answer_preview: params.answer.slice(0, 500),
    rating: params.rating,
    feedback_text: params.feedbackText,
  });
}

export function trackError(params: {
  query: string;
  errorType: string;
  errorMessage: string;
}) {
  posthog.capture('ask_baml_error', {
    query: params.query,
    error_type: params.errorType,
    error_message: params.errorMessage,
  });
}

export function trackSessionEnded(params: {
  sessionId: string;
  messageCount: number;
  durationSeconds: number;
}) {
  posthog.capture('ask_baml_session_ended', {
    session_id: params.sessionId,
    message_count: params.messageCount,
    duration_seconds: params.durationSeconds,
  });
}
