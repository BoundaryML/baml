import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

/**
 * A page's content: uploaded as a file (.md or .html, which sets the format) or written below in
 * the chosen format. `defaultValue` and `defaultFormat` bring back the current page or a refused
 * draft.
 */
export function ContentField({
  defaultValue,
  defaultFormat = "markdown",
  rows = 12,
}: {
  defaultValue?: string;
  defaultFormat?: string;
  rows?: number;
}) {
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Label htmlFor="format">Format</Label>
        <select
          id="format"
          name="format"
          defaultValue={defaultFormat}
          className="h-9 rounded-md border bg-background px-3 text-sm"
        >
          <option value="markdown">Markdown (HTML allowed for illustrations)</option>
          <option value="html">A whole HTML page</option>
        </select>
      </div>
      <Label htmlFor="file">Upload a file</Label>
      <input
        id="file"
        name="file"
        type="file"
        accept=".md,.markdown,.html,.htm,text/markdown,text/html"
        className="block text-sm"
      />
      <p className="text-sm text-muted-foreground">
        A .md or .html file replaces what is written below. In Markdown, paste illustrations as raw
        HTML (inline SVG, a &lt;style&gt; block) with no blank lines inside them; ```mermaid blocks
        become diagrams.
      </p>
      <Label htmlFor="content">Or write it here</Label>
      <Textarea
        id="content"
        name="content"
        rows={rows}
        defaultValue={defaultValue}
        placeholder="# Title&#10;&#10;Why this change, and how it works…"
        className="font-mono text-xs"
      />
    </div>
  );
}
