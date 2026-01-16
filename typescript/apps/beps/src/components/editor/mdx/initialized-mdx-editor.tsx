"use client";

import type { ForwardedRef } from "react";
import {
  MDXEditor,
  type MDXEditorMethods,
  type MDXEditorProps,
  // Plugins
  headingsPlugin,
  listsPlugin,
  quotePlugin,
  thematicBreakPlugin,
  markdownShortcutPlugin,
  linkPlugin,
  linkDialogPlugin,
  tablePlugin,
  codeBlockPlugin,
  codeMirrorPlugin,
  frontmatterPlugin,
  diffSourcePlugin,
  toolbarPlugin,
  imagePlugin,
  // Toolbar components
  UndoRedo,
  BoldItalicUnderlineToggles,
  BlockTypeSelect,
  CreateLink,
  InsertImage,
  InsertTable,
  ListsToggle,
  InsertThematicBreak,
  CodeToggle,
  DiffSourceToggleWrapper,
  Separator,
  StrikeThroughSupSubToggles,
  InsertCodeBlock,
  ConditionalContents,
  ChangeCodeMirrorLanguage,
} from "@mdxeditor/editor";
import "@mdxeditor/editor/style.css";

interface InitializedMDXEditorProps extends MDXEditorProps {
  editorRef: ForwardedRef<MDXEditorMethods> | null;
  editable?: boolean;
  showToolbar?: boolean;
  diffMarkdown?: string;
}

export default function InitializedMDXEditor({
  editorRef,
  editable = true,
  showToolbar = true,
  diffMarkdown,
  ...props
}: InitializedMDXEditorProps) {
  const plugins = [
    headingsPlugin(),
    listsPlugin(),
    quotePlugin(),
    thematicBreakPlugin(),
    markdownShortcutPlugin(),
    linkPlugin(),
    linkDialogPlugin(),
    tablePlugin(),
    imagePlugin(),
    frontmatterPlugin(),
    codeBlockPlugin({ defaultCodeBlockLanguage: "baml" }),
    codeMirrorPlugin({
      codeBlockLanguages: {
        baml: "TypeScript",
        typescript: "TypeScript",
        javascript: "JavaScript",
        python: "Python",
        rust: "Rust",
        go: "Go",
        json: "JSON",
        yaml: "YAML",
        bash: "Bash",
        shell: "Shell",
        sql: "SQL",
        html: "HTML",
        css: "CSS",
        "": "Plain Text",
      },
    }),
    diffSourcePlugin({
      viewMode: "rich-text",
      diffMarkdown: diffMarkdown,
    }),
    ...(showToolbar && editable
      ? [
          toolbarPlugin({
            toolbarContents: () => (
              <>
                <UndoRedo />
                <Separator />
                <BoldItalicUnderlineToggles />
                <CodeToggle />
                <Separator />
                <StrikeThroughSupSubToggles />
                <Separator />
                <ListsToggle />
                <Separator />
                <BlockTypeSelect />
                <Separator />
                <CreateLink />
                <InsertImage />
                <InsertTable />
                <InsertThematicBreak />
                <Separator />
                <InsertCodeBlock />
                <ConditionalContents
                  options={[
                    {
                      when: (editor) => editor?.editorType === "codeblock",
                      contents: () => <ChangeCodeMirrorLanguage />,
                    },
                  ]}
                />
                <Separator />
                <DiffSourceToggleWrapper options={["rich-text", "source"]}>
                  <span className="text-xs">Source</span>
                </DiffSourceToggleWrapper>
              </>
            ),
          }),
        ]
      : []),
  ];

  return (
    <MDXEditor
      plugins={plugins}
      {...props}
      ref={editorRef}
      readOnly={!editable}
      contentEditableClassName={`
        prose prose-sm sm:prose-base lg:prose-lg dark:prose-invert max-w-none
        min-h-[200px] focus:outline-none
        prose-code:before:content-none prose-code:after:content-none
        ${!editable ? "cursor-default" : ""}
      `}
    />
  );
}
