import { cookies } from 'next/headers';
import type { ReactNode } from 'react';
import { BookPerspectiveProvider } from '@/components/book-perspective-provider';
import {
  BOOK_PERSPECTIVE_COOKIE,
  parseBookPerspective,
} from '@/lib/content/book-perspectives';

export default async function BookLayout({
  children,
}: {
  children: ReactNode;
}) {
  const preference = (await cookies()).get(BOOK_PERSPECTIVE_COOKIE)?.value;
  return (
    <BookPerspectiveProvider
      initialPreference={parseBookPerspective(preference) ?? 'default'}
    >
      {children}
    </BookPerspectiveProvider>
  );
}
