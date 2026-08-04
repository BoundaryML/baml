declare module "*.mdx" {
  // Compiled MDX prompt component (rendered to markdown text by the Task runtime).
  const MDXComponent: (props: Record<string, unknown>) => unknown;
  export default MDXComponent;
}
