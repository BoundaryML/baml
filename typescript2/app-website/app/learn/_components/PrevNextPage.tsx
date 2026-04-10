import { useAtom, useAtomValue } from 'jotai';
import { useEffect } from 'react';
import {
  lessonsAtom,
  lessonAndPageIdAtom,
  safeCurrentIndicesAtom,
} from '../lib/learn-atoms';

export default function PrevNextPage() {
  let [lessonAndPageId, setLessonAndPageId] = useAtom(lessonAndPageIdAtom);
  let { lesson, page } = useAtomValue(safeCurrentIndicesAtom); // Get safe values
  let lessons = useAtomValue(lessonsAtom);

  // Add keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input, textarea, or contenteditable
      const target = e.target as HTMLElement;
      if (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable ||
        target.closest('.cm-editor')
      ) {
        // Also ignore if in CodeMirror editor
        return;
      }

      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        previousPage();
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        nextPage();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [lessonAndPageId, lessons, lesson]); // Dependencies for the functions used inside

  // Fallback protection in case safe indices still fail
  if (!lesson || !page) {
    return <div>Loading...</div>;
  }

  let pagesInLesson = lesson.pages.length;

  let linkStyle = 'text-blue-500 cursor-pointer select-none';

  function previousPage() {
    if (lessons && lesson && lessonAndPageId.pageIndex > 0) {
      const newPageIndex = lessonAndPageId.pageIndex - 1;
      const newPage = lesson.pages[newPageIndex];
      setLessonAndPageId({
        lessonIndex: lessonAndPageId.lessonIndex,
        pageIndex: newPageIndex,
        tabId: newPage?.defaultTab || '',
      });
    } else {
      // If we're on the first page of a lesson, go to the previous lesson
      if (lessonAndPageId.lessonIndex > 0) {
        let newLessonIndex = lessonAndPageId.lessonIndex - 1;
        const newLesson = lessons[newLessonIndex];
        // Go to the last page of the previous lesson
        const lastPageIndex = newLesson?.pages ? newLesson.pages.length - 1 : 0;
        const lastPage = newLesson?.pages?.[lastPageIndex];
        setLessonAndPageId({
          lessonIndex: newLessonIndex,
          pageIndex: lastPageIndex,
          tabId: lastPage?.defaultTab || '',
        });
      }
    }
  }

  function nextPage() {
    const onLastLesson = lessonAndPageId.lessonIndex === lessons.length - 1;
    const onLastPageOfLesson =
      lesson && lessonAndPageId.pageIndex === lesson.pages.length - 1;
    if (onLastPageOfLesson) {
      if (!onLastLesson) {
        const newLessonIndex = Math.min(
          lessons.length - 1,
          lessonAndPageId.lessonIndex + 1,
        );
        const newLesson = lessons[newLessonIndex];
        setLessonAndPageId({
          lessonIndex: newLessonIndex,
          pageIndex: 0,
          tabId: newLesson?.pages?.[0]?.defaultTab || '',
        });
      }
    } else {
      const newPageIndex = Math.min(
        (lesson?.pages?.length || 100) - 1,
        lessonAndPageId.pageIndex + 1,
      );
      const newPage = lesson?.pages?.[newPageIndex];
      setLessonAndPageId({
        lessonIndex: lessonAndPageId.lessonIndex,
        pageIndex: newPageIndex,
        tabId: newPage?.defaultTab || '',
      });
    }
  }

  return (
    <div className="flex flex-row gap-2 w-full justify-center text-2xl">
      <span className={linkStyle} onClick={() => previousPage()}>
        &lt;
      </span>
      <span className="w-2">{lessonAndPageId.pageIndex + 1}</span>{' '}
      <span className="w-2">/</span>{' '}
      <span className="w-2">{pagesInLesson}</span>
      <span className={linkStyle} onClick={() => nextPage()}>
        &gt;
      </span>
    </div>
  );
}
