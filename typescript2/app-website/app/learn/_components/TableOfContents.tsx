import {
  currentLessonAndPage,
  lessonAndPageIdAtom,
  lessonsAtom,
} from '../lib/learn-atoms';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';

export default function TableOfContents() {
  let [lessonAndPage] = useAtom(currentLessonAndPage);
  let [lessonAndPageId, setLessonAndPageId] = useAtom(lessonAndPageIdAtom);
  let lessons = useAtomValue(lessonsAtom);

  function gotoLesson(lessonId: number) {
    if (lessonId >= 0 && lessonId < lessons.length) {
      setLessonAndPageId({
        lessonIndex: lessonId,
        pageIndex: 0,
        tabId: lessons[lessonId]?.pages?.[0]?.defaultTab || '',
      });
    }
  }

  function gotoPage(lessonId: number, pageId: number) {
    if (
      lessons &&
      lessonId >= 0 &&
      lessonId < lessons.length &&
      pageId >= 0 &&
      lessons[lessonId] &&
      pageId < lessons[lessonId].pages?.length
    ) {
      setLessonAndPageId({
        lessonIndex: lessonId,
        pageIndex: pageId,
        tabId: lessons[lessonId]?.pages?.[pageId]?.defaultTab || '',
      });
    }
  }

  const extraLessonStyle = (index: number) =>
    index === lessonAndPageId.lessonIndex ? 'bg-blue-100' : '';
  const extraPageStyle = (lessonIndex: number, pageIndex: number) =>
    pageIndex === lessonAndPageId.pageIndex &&
    lessonIndex == lessonAndPageId.lessonIndex
      ? 'bg-blue-200'
      : '';
  return (
    <div className="p-2">
      {lessons.map((lesson, index) => (
        <div key={`lesson-${index}-${lesson.title}`}>
          <h1
            className={`cursor-pointer text-lg ${extraLessonStyle(index)}`}
            onClick={() => gotoLesson(index)}
          >
            {lesson.title}
          </h1>
          {lesson.pages.map((page, pageIndex) => (
            <div
              key={`page-${pageIndex}-${page.title}`}
              className={`cursor-pointer pl-2 ${extraPageStyle(index, pageIndex)}`}
            >
              <h2 onClick={() => gotoPage(index, pageIndex)}>{page.title}</h2>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
