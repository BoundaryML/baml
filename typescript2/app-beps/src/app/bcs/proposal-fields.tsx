import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

/** The page's HTML, uploaded as a file or pasted; `defaultValue` brings back a refused draft. */
export function HtmlField({ defaultValue }: { defaultValue?: string }) {
  return (
    <div className="space-y-2">
      <Label htmlFor="file">The page</Label>
      <input
        id="file"
        name="file"
        type="file"
        accept=".html,text/html"
        className="block text-sm"
      />
      <p className="text-sm text-muted-foreground">
        An HTML file, such as a design doc an agent made, with its styles and diagrams inline.
      </p>
      <Label htmlFor="html">Or paste the HTML</Label>
      <Textarea
        id="html"
        name="html"
        rows={8}
        defaultValue={defaultValue}
        placeholder="<!doctype html>..."
        className="font-mono text-xs"
      />
    </div>
  );
}
