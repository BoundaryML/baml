// Resizable panel layout. This is the library that shadcn's `Resizable`
// wraps; the app has no Tailwind or shadcn setup, so it is used directly.
import type { ReactElement, ReactNode } from "react";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";

export interface SplitPane {
  id: string;
  /** Percent of the group, as a string ("28"), or pixels as a number. */
  defaultSize: string | number;
  minSize?: string | number;
  content: ReactNode;
}

/** Layouts are remembered per group id. Storage can be unavailable (private
 * windows, blocked site data); the layout then falls back to the defaults. */
function layoutStorage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

export function Split({ id, orientation, panes }: { id: string; orientation: "horizontal" | "vertical"; panes: SplitPane[] }): ReactElement {
  const storage = layoutStorage();
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: `durable-demo:${id}`,
    panelIds: panes.map((pane) => pane.id),
    ...(storage ? { storage } : {}),
  });
  return (
    <Group id={id} className="split" orientation={orientation} defaultLayout={defaultLayout} onLayoutChanged={onLayoutChanged}>
      {panes.flatMap((pane, index) => [
        ...(index > 0
          ? [<Separator key={`${pane.id}:sep`} className="split-sep" data-orientation={orientation} />]
          : []),
        <Panel key={pane.id} id={pane.id} className="split-pane" defaultSize={pane.defaultSize} minSize={pane.minSize ?? "10"}>
          {pane.content}
        </Panel>,
      ])}
    </Group>
  );
}
