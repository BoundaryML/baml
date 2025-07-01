import { Tabs, TabsList, TabsTrigger, TabsContent } from '@baml/ui/tabs';
import { Button } from '@baml/ui/button';
import { BarChart2 } from 'lucide-react';
import { useAtom } from 'jotai';
import { PromptPreviewCurl } from './prompt-preview-curl';
import { PromptPreviewContent } from './prompt-preview-content';
import { ClientGraphView } from './test-panel/components/ClientGraphView';
import { displaySettingsAtom } from '../preview-toolbar';

export const PromptRenderWrapper = () => {
  const [displaySettings, setDisplaySettings] = useAtom(displaySettingsAtom);

  return (
    <Tabs defaultValue="preview">
      <div className="flex items-center justify-between">
        <TabsList>
          <TabsTrigger value="preview">Preview</TabsTrigger>
          <TabsTrigger value="curl">cURL</TabsTrigger>
          <TabsTrigger value="client-graph">Client Graph</TabsTrigger>
        </TabsList>
        <Button
          variant="ghost"
          size="sm"
          className="flex items-center gap-2 text-muted-foreground/70 hover:text-foreground"
          onClick={() =>
            setDisplaySettings((prev) => ({
              ...prev,
              showTokens: !prev.showTokens,
            }))
          }
        >
          <BarChart2 className="size-4" />
          <span className="text-sm">
            {displaySettings.showTokens ? 'Hide Tokens' : 'Show Tokens'}
          </span>
        </Button>
      </div>
      <TabsContent value="preview">
        <PromptPreviewContent />
      </TabsContent>
      <TabsContent value="curl">
        <PromptPreviewCurl />
      </TabsContent>
      <TabsContent value="client-graph">
        <ClientGraphView />
      </TabsContent>
    </Tabs>
  );
};
