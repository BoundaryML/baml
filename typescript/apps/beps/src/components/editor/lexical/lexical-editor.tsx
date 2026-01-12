"use client";

import { useEffect, useImperativeHandle, forwardRef, useRef, useCallback } from 'react';
import { LexicalComposer } from '@lexical/react/LexicalComposer';
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin';
import { ContentEditable } from '@lexical/react/LexicalContentEditable';
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin';
import { OnChangePlugin } from '@lexical/react/LexicalOnChangePlugin';
import { ListPlugin } from '@lexical/react/LexicalListPlugin';
import { MarkdownShortcutPlugin } from '@lexical/react/LexicalMarkdownShortcutPlugin';
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $getRoot, LexicalEditor } from 'lexical';
import { ListNode, ListItemNode } from '@lexical/list';
import { HeadingNode, QuoteNode } from '@lexical/rich-text';
import { CodeNode, CodeHighlightNode } from '@lexical/code';
import { LinkNode } from '@lexical/link';
import { HorizontalRuleNode } from '@lexical/react/LexicalHorizontalRuleNode';
import {
  TRANSFORMERS,
  $convertFromMarkdownString,
  $convertToMarkdownString,
} from '@lexical/markdown';

export interface LexicalEditorHandle {
  getMarkdown: () => string;
  getEditor: () => LexicalEditor | null;
}

interface LexicalEditorProps {
  initialContent: string;
  onChange?: (markdown: string) => void;
  editable?: boolean;
  placeholder?: string;
  className?: string;
}

// Theme for styling
const theme = {
  paragraph: 'mb-2',
  heading: {
    h1: 'text-3xl font-bold mb-4',
    h2: 'text-2xl font-bold mb-3',
    h3: 'text-xl font-bold mb-2',
    h4: 'text-lg font-bold mb-2',
    h5: 'text-base font-bold mb-1',
    h6: 'text-sm font-bold mb-1',
  },
  list: {
    ul: 'list-disc ml-6 mb-2',
    ol: 'list-decimal ml-6 mb-2',
    listitem: 'mb-1',
    nested: {
      listitem: 'list-none',
    },
  },
  quote: 'border-l-4 border-gray-300 pl-4 italic my-2',
  code: 'bg-gray-100 dark:bg-gray-800 rounded p-4 font-mono text-sm overflow-x-auto my-2',
  text: {
    bold: 'font-bold',
    italic: 'italic',
    strikethrough: 'line-through',
    code: 'bg-gray-100 dark:bg-gray-800 px-1 rounded font-mono text-sm',
  },
  link: 'text-blue-600 dark:text-blue-400 underline',
};

function onError(error: Error): void {
  console.error('Lexical error:', error);
}

// Inner component that has access to editor context
function EditorInner({
  onChange,
  editable,
  initialContent,
  editorRef,
}: {
  onChange?: (markdown: string) => void;
  editable: boolean;
  initialContent: string;
  editorRef: React.MutableRefObject<LexicalEditor | null>;
}) {
  const [editor] = useLexicalComposerContext();
  const isInitialized = useRef(false);
  const lastContentRef = useRef<string | null>(null);

  // Store editor reference
  useEffect(() => {
    editorRef.current = editor;
  }, [editor, editorRef]);

  // Set editable state
  useEffect(() => {
    editor.setEditable(editable);
  }, [editor, editable]);

  // Initialize content from markdown on first mount
  useEffect(() => {
    if (!isInitialized.current) {
      isInitialized.current = true;
      lastContentRef.current = initialContent;
      if (initialContent) {
        editor.update(() => {
          const root = $getRoot();
          root.clear();
          $convertFromMarkdownString(initialContent, TRANSFORMERS, root);
        });
      }
    }
  }, [editor, initialContent]);

  // Handle external content changes (e.g., switching pages)
  useEffect(() => {
    if (isInitialized.current && initialContent !== lastContentRef.current) {
      lastContentRef.current = initialContent;
      editor.update(() => {
        const root = $getRoot();
        root.clear();
        if (initialContent) {
          $convertFromMarkdownString(initialContent, TRANSFORMERS, root);
        }
      });
    }
  }, [editor, initialContent]);

  // Handle changes
  const handleChange = useCallback(
    () => {
      if (onChange && isInitialized.current) {
        editor.getEditorState().read(() => {
          const markdown = $convertToMarkdownString(TRANSFORMERS);
          if (markdown !== lastContentRef.current) {
            lastContentRef.current = markdown;
            onChange(markdown);
          }
        });
      }
    },
    [editor, onChange]
  );

  return (
    <>
      <OnChangePlugin onChange={handleChange} />
      <HistoryPlugin />
      <ListPlugin />
      <MarkdownShortcutPlugin transformers={TRANSFORMERS} />
    </>
  );
}

export const LexicalEditorComponent = forwardRef<LexicalEditorHandle, LexicalEditorProps>(
  function LexicalEditorComponent(
    {
      initialContent,
      onChange,
      editable = false,
      placeholder = 'Start writing...',
      className,
    },
    ref
  ) {
    const editorRef = useRef<LexicalEditor | null>(null);

    const initialConfig = {
      namespace: 'BepEditor',
      theme,
      onError,
      editable,
      nodes: [
        HeadingNode,
        QuoteNode,
        CodeNode,
        CodeHighlightNode,
        ListNode,
        ListItemNode,
        LinkNode,
        HorizontalRuleNode,
      ],
    };

    // Expose methods via ref
    useImperativeHandle(ref, () => ({
      getMarkdown: () => {
        if (!editorRef.current) return '';
        let markdown = '';
        editorRef.current.getEditorState().read(() => {
          markdown = $convertToMarkdownString(TRANSFORMERS);
        });
        return markdown;
      },
      getEditor: () => editorRef.current,
    }));

    return (
      <LexicalComposer initialConfig={initialConfig}>
        <div className="relative">
          <RichTextPlugin
            contentEditable={
              <ContentEditable
                className={`prose prose-sm sm:prose-base lg:prose-lg dark:prose-invert max-w-none focus:outline-none min-h-[200px] ${className ?? ''}`}
              />
            }
            placeholder={
              <div className="absolute top-0 left-0 text-gray-400 pointer-events-none">
                {placeholder}
              </div>
            }
            ErrorBoundary={LexicalErrorBoundary}
          />
          <EditorInner
            onChange={onChange}
            editable={editable}
            initialContent={initialContent}
            editorRef={editorRef}
          />
        </div>
      </LexicalComposer>
    );
  }
);
