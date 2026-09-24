'use client';

import { useSearchParams } from 'next/navigation';
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useState,
} from 'react';
import {
  BOOK_PERSPECTIVE_COOKIE,
  BOOK_PERSPECTIVE_PARAM,
  type BookPerspective,
  parseBookPerspective,
  perspectiveHref,
} from '@/lib/content/book-perspectives';

const BookPerspectiveContext = createContext<{
  requested: BookPerspective;
  save: (perspective: BookPerspective) => void;
} | null>(null);

export function BookPerspectiveProvider({
  children,
  initialPreference,
}: {
  children: ReactNode;
  initialPreference: BookPerspective;
}) {
  const [preference, setPreference] = useState(initialPreference);
  const searchParams = useSearchParams();
  const requested =
    parseBookPerspective(searchParams.get(BOOK_PERSPECTIVE_PARAM)) ??
    preference;
  useEffect(() => {
    if (!parseBookPerspective(searchParams.get(BOOK_PERSPECTIVE_PARAM))) {
      window.history.replaceState(
        null,
        '',
        perspectiveHref(window.location.href, requested),
      );
    }
  }, [requested, searchParams]);
  return (
    <BookPerspectiveContext.Provider
      value={{
        requested,
        save: (perspective) => {
          // biome-ignore lint/suspicious/noDocumentCookie: SSR needs a cookie; document.cookie also works on non-HTTPS local previews.
          document.cookie = `${BOOK_PERSPECTIVE_COOKIE}=${perspective}; Path=/baml/book; Max-Age=31536000; SameSite=Lax${location.protocol === 'https:' ? '; Secure' : ''}`;
          setPreference(perspective);
        },
      }}
    >
      {children}
    </BookPerspectiveContext.Provider>
  );
}

export function useBookPerspective() {
  const value = useContext(BookPerspectiveContext);
  if (!value) throw new Error('Book content requires BookPerspectiveProvider');
  return value;
}

export function useOptionalBookPerspective() {
  return useContext(BookPerspectiveContext);
}
