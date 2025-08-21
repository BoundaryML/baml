'use client';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@baml/ui/dropdown-menu';
import { Button } from '@baml/ui/button';
import { Settings } from 'lucide-react';
import { useAtom } from 'jotai';
import { betaFeatureEnabledAtom } from '@baml/playground-common';

export function SettingsDropdown() {
  const [betaFeatureEnabled, setBetaFeatureEnabled] = useAtom(betaFeatureEnabledAtom);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="opacity-40 hover:opacity-100 h-full w-fit"
        >
          <Settings size={18} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-fit p-0">
        <DropdownMenuLabel className="text-xs px-2 py-1.5">
          Experimental Features
        </DropdownMenuLabel>
        <DropdownMenuCheckboxItem
          checked={betaFeatureEnabled}
          onCheckedChange={setBetaFeatureEnabled}
          className="text-sm px-2 py-1.5"
        >
          Beta Features
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <div className="px-2 py-1 text-xs text-muted-foreground">
          Enable experimental BAML features
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}