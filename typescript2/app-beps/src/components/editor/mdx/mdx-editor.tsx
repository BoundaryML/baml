"use client";

import dynamic from "next/dynamic";
import { forwardRef, useCallback, useImperativeHandle, useRef } from "react";
import { type MDXEditorMethods, type MDXEditorProps } from "@mdxeditor/editor";

// Dynamic import with SSR disabled - required for MDXEditor in Next.js
const Editor = dynamic(() => import("./initialized-mdx-editor"), {
  ssr: false,
  loading: () => (
    <div className="min-h-[200px] flex items-center justify-center text-muted-foreground">
      Loading editor...
    </div>
  ),
});

export interface MDXEditorHandle {
  getMarkdown: () => string;
  setMarkdown: (markdown: string) => void;
  getRootElement: () => HTMLElement | null;
}

interface MDXEditorComponentProps {
  initialContent: string;
  onChange?: (markdown: string) => void;
  editable?: boolean;
  placeholder?: string;
  className?: string;
  showToolbar?: boolean;
  diffMarkdown?: string;
}

export const MDXEditorComponent = forwardRef<MDXEditorHandle, MDXEditorComponentProps>(
  function MDXEditorComponent(
    {
      initialContent,
      onChange,
      editable = false,
      placeholder = "Start writing...",
      className,
      showToolbar = true,
      diffMarkdown,
    },
    ref
  ) {
    const editorRef = useRef<MDXEditorMethods>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    // Expose methods via ref
    useImperativeHandle(ref, () => ({
      getMarkdown: () => editorRef.current?.getMarkdown() ?? "",
      setMarkdown: (markdown: string) => editorRef.current?.setMarkdown(markdown),
      getRootElement: () => {
        // Find the contentEditable element inside the editor
        return containerRef.current?.querySelector('[contenteditable="true"]') as HTMLElement | null;
      },
    }));

    const handleChange = useCallback(
      (markdown: string) => {
        onChange?.(markdown);
      },
      [onChange]
    );

    return (
      <div ref={containerRef} className={className}>
        <Editor
          editorRef={editorRef}
          markdown={initialContent}
          onChange={handleChange}
          editable={editable}
          placeholder={placeholder}
          showToolbar={showToolbar}
          diffMarkdown={diffMarkdown}
        />
      </div>
    );
  }
);

// Re-export for convenience
export type { MDXEditorMethods, MDXEditorProps };

