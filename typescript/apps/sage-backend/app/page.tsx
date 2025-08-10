'use client';
import type { QueryRequest, QueryResponse } from '@baml/sage-interface';
import { useEffect, useState } from 'react';
import { submitQuery } from './actions/query';
import { PLACEHOLDER_QUERIES } from './eval-data';
async function hashQueryRequest(request: QueryRequest): Promise<string> {
  const sortedRequest = JSON.stringify(request, Object.keys(request).sort());
  const encoder = new TextEncoder();
  const data = encoder.encode(sortedRequest);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}

// Browser localStorage cache utilities
const CACHE_PREFIX = 'query_cache_';

function getCachedResult(cacheKey: string): QueryResponse | null {
  try {
    const cached = localStorage.getItem(CACHE_PREFIX + cacheKey);
    return cached ? JSON.parse(cached) : null;
  } catch {
    return null;
  }
}

function setCachedResult(cacheKey: string, result: QueryResponse): void {
  try {
    localStorage.setItem(CACHE_PREFIX + cacheKey, JSON.stringify(result));
  } catch {
    // localStorage might be full or unavailable, ignore silently
  }
}

async function cachedSubmitQuery({
  queryRequest,
  allowCache,
}: {
  queryRequest: QueryRequest;
  allowCache: boolean;
}): Promise<QueryResponse> {
  const cacheKey = await hashQueryRequest(queryRequest);

  if (allowCache) {
    // Check if result is already cached in localStorage
    const cachedResult = getCachedResult(cacheKey);
    if (cachedResult) {
      return cachedResult;
    }
  }

  // If not cached, call the original function and cache the result
  const result = await submitQuery(queryRequest);
  setCachedResult(cacheKey, result);

  return result;
}

interface QueryResult {
  queryData: { input: QueryRequest; inspectNotes?: string };
  response: QueryResponse | null;
  error: string | null;
  isLoading: boolean;
}

export default function Home() {
  const [queryResults, setQueryResults] = useState<QueryResult[]>(
    PLACEHOLDER_QUERIES.map((queryData) => ({
      queryData,
      response: null,
      error: null,
      isLoading: false,
    })),
  );

  // Function to run a single query
  const runQuery = async ({
    queryIndex,
    allowCache,
  }: { queryIndex: number; allowCache: boolean }) => {
    setQueryResults((prev) =>
      prev.map((item, i) =>
        i === queryIndex ? { ...item, isLoading: true, error: null, response: null } : item,
      ),
    );

    try {
      let response: QueryResponse;

      if (allowCache) {
        // Use cached version
        response = await cachedSubmitQuery({
          queryRequest: PLACEHOLDER_QUERIES[queryIndex].input,
          allowCache,
        });
      } else {
        response = await cachedSubmitQuery({
          queryRequest: PLACEHOLDER_QUERIES[queryIndex].input,
          allowCache,
        });
      }

      setQueryResults((prev) =>
        prev.map((item, i) => (i === queryIndex ? { ...item, response, isLoading: false } : item)),
      );
    } catch (err) {
      setQueryResults((prev) =>
        prev.map((item, i) =>
          i === queryIndex
            ? {
                ...item,
                error: err instanceof Error ? err.message : 'An error occurred',
                isLoading: false,
              }
            : item,
        ),
      );
    }
  };

  // Function to retry all queries
  const retryAllQueries = async () => {
    await Promise.all(
      PLACEHOLDER_QUERIES.map((_, index) => runQuery({ queryIndex: index, allowCache: false })),
    );
  };

  // Run all queries on mount
  useEffect(() => {
    Promise.all(
      PLACEHOLDER_QUERIES.map((_, index) => runQuery({ queryIndex: index, allowCache: true })),
    );
  }, []);

  return (
    <div className="font-sans min-h-screen p-8 pb-20 sm:p-20">
      <main className="w-full mx-auto">
        <h1 className="text-3xl font-bold mb-8 text-gray-900 dark:text-gray-100">
          BAML Query Test Results
        </h1>

        <table className="w-full border-collapse bg-white dark:bg-gray-800 rounded-lg">
          <thead>
            <tr className="border-b border-gray-200 dark:border-gray-700">
              <th className="px-4 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100">
                <div className="flex flex-col gap-1">
                  <button
                    onClick={retryAllQueries}
                    className="text-xs px-2 py-1 bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors"
                  >
                    Retry All
                  </button>
                </div>
              </th>
              <th className="px-6 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100 w-72">
                Query
              </th>
              <th className="px-6 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100">
                Answer
              </th>
              <th className="px-6 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100 w-48">
                Related Documents
              </th>
              <th className="px-6 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100 w-48">
                Suggested Messages
              </th>
              <th className="px-6 py-4 text-left text-sm font-semibold text-gray-900 dark:text-gray-100 w-96">
                Inspect Notes
              </th>
            </tr>
          </thead>
          <tbody>
            {queryResults.map((item, index) => (
              <tr
                key={index}
                className={`border-b border-gray-200 dark:border-gray-700 hover:bg-blue-50 dark:hover:bg-blue-900/20 ${
                  index % 2 === 0 ? 'bg-white dark:bg-gray-800' : 'bg-gray-100 dark:bg-gray-700'
                }`}
              >
                <td className="px-4 py-4 align-middle">
                  <button
                    onClick={() => runQuery({ queryIndex: index, allowCache: false })}
                    disabled={item.isLoading}
                    className="text-xs px-3 py-1 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white rounded transition-colors"
                  >
                    {item.isLoading ? 'Loading...' : 'Retry'}
                  </button>
                </td>
                <td className="px-6 py-4 align-middle">
                  <p className="text-sm text-gray-700 dark:text-gray-300">
                    {item.queryData.input.message.text}
                  </p>
                </td>
                <td className="px-6 py-4">
                  {item.isLoading && (
                    <div className="flex items-center space-x-2">
                      <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-600"></div>
                      <span className="text-sm text-gray-600 dark:text-gray-400">Loading...</span>
                    </div>
                  )}

                  {item.error && (
                    <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded">
                      <p className="text-sm text-red-800 dark:text-red-200">{item.error}</p>
                    </div>
                  )}

                  {item.response && item.response.message.text && (
                    <div className="p-3 bg-green-50 dark:bg-green-900/20 rounded">
                      <p className="text-sm text-gray-800 dark:text-gray-200 whitespace-pre-wrap">
                        {item.response.message.text}
                      </p>
                    </div>
                  )}
                </td>
                <td className="px-6 py-4 max-w-96">
                  {item.response?.message.ranked_docs &&
                    item.response.message.ranked_docs.length > 0 && (
                      <div className="max-w-96">
                        <div>
                          {item.response.message.ranked_docs.map((doc: any) => (
                            <div
                              key={doc.url}
                              className="p-2 bg-gray-50 dark:bg-gray-700/50 rounded text-xs mb-1"
                            >
                              <div className="flex items-center justify-between gap-2">
                                <div className="flex-1 min-w-0">
                                  <p className="font-medium text-gray-700 dark:text-gray-300 truncate text-xs">
                                    {doc.title}
                                  </p>
                                  <a
                                    href={doc.url}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="text-blue-600 dark:text-blue-400 hover:underline truncate block text-xs"
                                  >
                                    {doc.url}
                                  </a>
                                </div>
                                <span
                                  className={`flex-shrink-0 px-2 py-0.5 rounded text-xs font-medium ${
                                    doc.relevance === 'very-relevant'
                                      ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300'
                                      : doc.relevance === 'relevant'
                                        ? 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300'
                                        : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
                                  }`}
                                >
                                  {doc.relevance}
                                </span>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                </td>
                <td className="px-6 py-4">
                  {item.response?.message.suggested_messages &&
                    item.response.message.suggested_messages.length > 0 && (
                      <div>
                        <div>
                          {item.response.message.suggested_messages.map(
                            (message: any, msgIndex: any) => (
                              <div
                                key={msgIndex}
                                className="p-2 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded text-xs mb-1"
                              >
                                <p className="text-blue-800 dark:text-blue-200">{message}</p>
                              </div>
                            ),
                          )}
                        </div>
                      </div>
                    )}
                </td>
                <td className="px-6 py-4 align-middle">
                  {item.queryData.inspectNotes && (
                    <div className="p-2 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded">
                      <p className="text-xs text-yellow-800 dark:text-yellow-200">
                        {item.queryData.inspectNotes}
                      </p>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </main>
    </div>
  );
}
