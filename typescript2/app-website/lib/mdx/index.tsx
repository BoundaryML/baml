import type { MDXComponents } from 'mdx/types';
import dynamic from 'next/dynamic';
import NextImage from 'next/image';
import NewsletterForm from '@/components/NewsletterForm';
import BamlBlock from './baml-block/BamlBlock2';
import { CodeBlocks } from './code-blocks';
import { DevSpotlight } from './dev-spotlight';
import { MDXMedia } from './media';
import { MDXNote } from './note';
import { ProblemStatement, TechniqueTitle, WhatIsSAP } from './sap/intro';
import { MDXTip } from './tip';
import Video from './video';

const BFCLDataComponent = dynamic(() => import('./bfcl/page'));

export const mdxComponents: MDXComponents = {
  BamlBlock: BamlBlock,
  BFCLDataComponent: BFCLDataComponent,
  CodeBlocks: CodeBlocks,
  Details: ({
    children,
    summary,
    ...props
  }: React.DetailedHTMLProps<
    React.DetailsHTMLAttributes<HTMLDetailsElement>,
    HTMLDetailsElement
  > & {
    summary: string;
  }) => (
    // Necessary due to a hydration error I can't quite figure out
    <details {...props}>
      {summary && <summary>{summary}</summary>}
      {children}
    </details>
  ),
  DevSpotlight: DevSpotlight,
  Image: NextImage,
  // TODO: re-enable once anchor tags are fixed in the app router
  // a: ({ children, ...props }) => {
  //   // check if external
  //   let te = false
  //   if (props.href?.startsWith('http')) {
  //     isExternal = true
  //   }

  //   return (
  //     // @ts-expect-error legacy refs
  //     <Link
  //       {...props}
  //       href={props.href || ''}
  //       target={isExternal ? '_blank' : undefined}
  //       rel={isExternal ? 'noopener noreferrer' : undefined}
  //     >
  //       {children}
  //     </Link>
  //   )
  // },
  // pre: ({
  //   children,
  //   ...props
  // }: React.DetailedHTMLProps<
  //   React.HTMLAttributes<HTMLElement>,
  //   HTMLPreElement
  // >) => {
  //   // TODO: extract title from children
  //   return (
  //     // @ts-expect-error RSC
  //     <Code {...props} theme="material-default">
  //       {children as any}
  //     </Code>
  //   );
  // },
  img: MDXMedia,
  NewsletterForm: NewsletterForm,
  Note: MDXNote,
  SapProblemStatement: ProblemStatement,
  SapTechniqueTitle: TechniqueTitle,
  SapWhatIsSAP: WhatIsSAP,
  Tip: MDXTip,
  Video: Video,
  // pre: ({ children, ...props }: React.DetailedHTMLProps<React.HTMLAttributes<HTMLElement>, HTMLPreElement>) => {
  //   // If we're inside a CodeBlocks component, just pass through
  //   if (Array.isArray(children) && children.length === 1 && typeof children[0] === 'object') {
  //     const child = children[0] as any
  //     // Preserve the filename from the original markdown
  //     if (child.props?.className) {
  //       const match = child.props.className.match(/language-.*?\s+filename="([^"]*)"/)
  //       if (match) {
  //         child.props['data-filename'] = match[1]
  //       }
  //     }
  //     return children
  //   }
  //   return <pre {...props}>{children}</pre>
  // },
  // code: ({ children, className, ...props }: { children: React.ReactNode; className?: string }) => {
  //   // Extract filename if it exists in the className
  //   const match = className?.match(/language-.*?\s+filename="([^"]*)"/)
  //   const filename = match ? match[1] : undefined
  //   const language = className?.replace(/\s+filename="[^"]*"/, '').replace('language-', '')

  //   return (
  //     <code className={language ? `language-${language}` : undefined} data-filename={filename} {...props}>
  //       {children}
  //     </code>
  //   )
  // },

  //   icons
  // InfoIcon: Info,
  // HomeIcon: Home,

  // file tree
  // FileTree,
  // File,
  // Folder,
};
