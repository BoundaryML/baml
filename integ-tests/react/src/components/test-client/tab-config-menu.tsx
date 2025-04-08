'use client';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  type ResponseCardConfig,
  useResponseCardConfigWithQueryParams,
} from '@/lib/store';
import type { OutputFormat } from '@/lib/store';
import { Settings } from 'lucide-react';

export function TabConfigMenu() {
  // Use the hook that syncs with URL query parameters
  const { config, updateConfig } = useResponseCardConfigWithQueryParams();

  // Type for tab IDs
  type TabId = 'data' | 'streamData' | 'finalData' | 'error';

  // List of available tabs
  const tabs = [
    { id: 'data' as TabId, label: 'Data Tab' },
    { id: 'streamData' as TabId, label: 'Stream Data Tab' },
    { id: 'finalData' as TabId, label: 'Final Data Tab' },
    { id: 'error' as TabId, label: 'Error Tab' },
  ];

  // Function to toggle a tab visibility
  const toggleTab = (tabId: TabId) => {
    // Create the property name for the config (e.g., showDataTab)
    const configKey =
      `show${tabId.charAt(0).toUpperCase() + tabId.slice(1)}Tab` as keyof ResponseCardConfig;

    // Count currently visible tabs
    const visibleTabsCount = [
      config.showDataTab,
      config.showStreamDataTab,
      config.showFinalDataTab,
      config.showErrorTab,
    ].filter(Boolean).length;

    // If we're trying to hide a tab and it's the last visible one, prevent it
    if (config[configKey] && visibleTabsCount <= 1) {
      // Don't allow hiding the last visible tab
      return;
    }

    // Check if we're hiding the current default tab
    const newValue = !config[configKey];
    const isHidingDefaultTab = !newValue && tabId === config.defaultTab;

    if (isHidingDefaultTab) {
      // Need to find a new default tab that will remain visible
      const newDefaultTab =
        tabs.find(
          (tab) =>
            tab.id !== tabId &&
            config[
              `show${tab.id.charAt(0).toUpperCase() + tab.id.slice(1)}Tab` as keyof ResponseCardConfig
            ],
        )?.id || 'data'; // Fallback to 'data' in case nothing is found (shouldn't happen)

      // Update both the tab visibility and the default tab
      updateConfig({
        [configKey]: newValue,
        defaultTab: newDefaultTab,
      });
    } else {
      // Just update the tab visibility
      updateConfig({
        [configKey]: newValue,
      });
    }
  };

  // Function to toggle display mode
  const toggleDisplayMode = (mode: 'tabs' | 'sections') => {
    updateConfig({
      displayMode: mode,
    });
  };

  // Function to update output format
  const toggleOutputFormat = (format: OutputFormat) => {
    updateConfig({
      outputFormat: format,
    });
  };

  // Function to update the default tab
  const setDefaultTab = (tabId: string) => {
    updateConfig({
      defaultTab: tabId,
    });
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          className="aspect-square h-9 w-9 p-0"
        >
          <Settings className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel>Configuration</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {/* Display Mode Selection */}
        <DropdownMenuLabel className="font-medium text-muted-foreground text-xs">
          Display Mode
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={config.displayMode}
          onValueChange={toggleDisplayMode as (value: string) => void}
        >
          <DropdownMenuRadioItem value="tabs">Tabs View</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="sections">
            Sections View
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        {/* Response Options */}
        <DropdownMenuLabel className="font-medium text-muted-foreground text-xs">
          Response Options
        </DropdownMenuLabel>
        <DropdownMenuCheckboxItem
          checked={config.isStreamingEnabled}
          onCheckedChange={(checked) =>
            updateConfig({ isStreamingEnabled: checked })
          }
        >
          Enable Streaming
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={config.showNetworkTimeline}
          onCheckedChange={(checked) =>
            updateConfig({ showNetworkTimeline: checked })
          }
        >
          Show LLM Timeline
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        {/* Output Format Selection */}
        <DropdownMenuLabel className="font-medium text-muted-foreground text-xs">
          Output Format
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={config.outputFormat}
          onValueChange={toggleOutputFormat as (value: string) => void}
        >
          <DropdownMenuRadioItem value="raw">Raw</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="json">JSON</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="yaml">YAML</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="markdown">
            Markdown
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        {/* Visible tabs checkboxes */}
        <DropdownMenuLabel className="font-medium text-muted-foreground text-xs">
          Visible Sections
        </DropdownMenuLabel>
        <DropdownMenuCheckboxItem
          checked={config.showDataTab}
          onCheckedChange={() => toggleTab('data')}
        >
          Data Tab
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={config.showStreamDataTab}
          onCheckedChange={() => toggleTab('streamData')}
        >
          Stream Data Tab
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={config.showFinalDataTab}
          onCheckedChange={() => toggleTab('finalData')}
        >
          Final Data Tab
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={config.showErrorTab}
          onCheckedChange={() => toggleTab('error')}
        >
          Error Tab
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        {/* Default tab selection */}
        <DropdownMenuLabel className="font-medium text-muted-foreground text-xs">
          Default Tab
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={config.defaultTab}
          onValueChange={setDefaultTab}
        >
          {tabs
            .filter(
              (tab) =>
                config[
                  `show${tab.id.charAt(0).toUpperCase() + tab.id.slice(1)}Tab` as keyof ResponseCardConfig
                ],
            )
            .map((tab) => (
              <DropdownMenuRadioItem key={tab.id} value={tab.id}>
                {tab.label}
              </DropdownMenuRadioItem>
            ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
