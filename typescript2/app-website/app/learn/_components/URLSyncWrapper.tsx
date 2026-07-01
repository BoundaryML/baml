'use client';

import { useEffect } from 'react';
import { useAtom, useAtomValue } from 'jotai';
import { useSearchParams, useRouter, usePathname } from 'next/navigation';
import { lessonsAtom, lessonAndPageIdAtom } from '../lib/learn-atoms';

export default function URLSyncWrapper({
  children,
}: {
  children: React.ReactNode;
}) {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const lessons = useAtomValue(lessonsAtom);
  const [lessonAndPageId, setLessonAndPageId] = useAtom(lessonAndPageIdAtom);

  // Read from URL on mount and when URL changes
  useEffect(() => {
    const lessonId = searchParams.get('lesson');
    const pageId = searchParams.get('page');

    if (lessonId && pageId && lessons.length > 0) {
      // Find lesson index by ID
      const lessonIndex = lessons.findIndex((l) => l.id === lessonId);
      if (lessonIndex !== -1) {
        const lesson = lessons[lessonIndex];
        if (!lesson) {
          console.error(`Lesson ${lessonId} not found`);
          return;
        }
        // Find page index by ID
        const pageIndex = lesson.pages.findIndex((p) => p.id === pageId);
        if (pageIndex !== -1) {
          const page = lesson.pages[pageIndex];
          if (!page) {
            console.error(`Page ${pageId} not found`);
            return;
          }
          setLessonAndPageId({
            lessonIndex,
            pageIndex,
            tabId: page.defaultTab || '',
          });
        }
      }
    }
  }, [searchParams, lessons, setLessonAndPageId]);

  // Update URL when lesson/page changes
  useEffect(() => {
    if (lessons.length > 0) {
      const lesson = lessons[lessonAndPageId.lessonIndex];
      const page = lesson?.pages[lessonAndPageId.pageIndex];

      if (lesson && page) {
        const params = new URLSearchParams();
        params.set('lesson', lesson.id);
        params.set('page', page.id);

        // Update URL without causing a navigation
        const newUrl = `${pathname}?${params.toString()}`;
        router.replace(newUrl, { scroll: false });
      }
    }
  }, [lessonAndPageId, lessons, router, pathname]);

  return <>{children}</>;
}
