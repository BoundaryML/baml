import { useAtom, useAtomValue } from 'jotai';
import {
  lessonAndPageIdAtom,
  safeCurrentIndicesAtom,
} from '../lib/learn-atoms';

import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '../../../components/ui/tabs';

export default function CodeTabs() {
  let [lessonAndPageId, setLessonAndPageId] = useAtom(lessonAndPageIdAtom);
  let { lesson, page } = useAtomValue(safeCurrentIndicesAtom); // Get safe values

  // Fallback protection in case safe indices still fail
  if (!lesson || !page || !page.fileContents) {
    return <div>Loading...</div>;
  }

  let files = Object.keys(page.fileContents).filter(
    (file) => !file.endsWith('_hidden.baml'),
  );

  return (
    <Tabs defaultValue={lessonAndPageId.tabId} value={lessonAndPageId.tabId}>
      <TabsList>
        {files.map((file, index) => (
          <TabsTrigger
            key={file}
            value={file}
            onClick={() => {
              setLessonAndPageId({
                lessonIndex: lessonAndPageId.lessonIndex,
                pageIndex: lessonAndPageId.pageIndex,
                tabId: file,
              });
            }}
          >
            {file}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}
