import { safeCurrentIndicesAtom } from '../lib/learn-atoms';

import { useAtomValue } from 'jotai';
import ReactMarkdown from 'react-markdown';

import { CodeEditor } from './CodeEditor';
import Header from './Header';
import PrevNextPage from './PrevNextPage';
import CodeTabs from './CodeTabs';
import OutputPanel from './OutputPanel';
import CodeTrigger from './CodeTrigger';

export default function LessonContent() {
  let { lesson, page } = useAtomValue(safeCurrentIndicesAtom); // Get safe values

  // Fallback protection
  if (!lesson || !page) {
    return <div>Loading...</div>;
  }

  let content = page.content || '';

  return (
    <div className="w-full h-[calc(100vh-80px)] flex flex-col">
      <Header />
      <div className="w-full bg-red-200 flex justify-center items-center flex-col">
        <h1 className="text-2xl font-bold">Under Construction</h1>
        <p>
          This learning resource is still a work in progress. Please check back
          soon!
        </p>
      </div>
      <div className="w-full flex-1 overflow-hidden flex flex-row">
        <div className="w-1/2 h-full p-4 flex flex-col">
          <div className="markdown-content prose prose-sm max-w-none flex-grow overflow-y-scroll min-h-0">
            <ReactMarkdown>{content}</ReactMarkdown>
          </div>
          <PrevNextPage />
        </div>
        <div className="w-1/2 h-full flex flex-col">
          <CodeTabs />
          <div className="w-full h-full flex flex-grow flex-1 overflow-y-scroll">
            <CodeEditor />
          </div>
          <CodeTrigger />
          <div className="w-full p-5 bg-slate-100">
            <OutputPanel />
          </div>
        </div>
      </div>
    </div>
  );
}
