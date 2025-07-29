'use client';

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@baml/ui/collapsible';
/* eslint-disable @typescript-eslint/no-floating-promises */
import { cn } from '@baml/ui/lib/utils';
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarRail,
} from '@baml/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  FlaskConical,
  FunctionSquare,
  Play,
} from 'lucide-react';
import * as React from 'react';
import { selectedItemAtom } from '../atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import {
  functionsAreStaleAtom,
  functionsAtom,
  isSidebarOpenAtom,
} from './atoms';
import { SearchForm } from './search-form';
import { TestItem } from './test-item';
import type { FunctionData } from './types';

export { isSidebarOpenAtom };

export function TestingSidebar() {
  const functions = useAtomValue(functionsAtom);
  const [searchTerm, setSearchTerm] = React.useState('');
  const [openCollapsibles, setOpenCollapsibles] = React.useState<Set<string>>(
    new Set(),
  );
  const runBamlTests = useRunBamlTests();
  const functionsAreStale = useAtomValue(functionsAreStaleAtom);
  const selectedItem = useAtomValue(selectedItemAtom);
  const setSelectedItem = useSetAtom(selectedItemAtom);

  const filteredFunctions = functions.filter(
    (func: FunctionData) =>
      func.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      func.tests.some((test) =>
        test.toLowerCase().includes(searchTerm.toLowerCase()),
      ),
  );

  const handleRunFilteredTests = () => {
    const testsToRun = filteredFunctions.flatMap((func) =>
      func.tests.map((test) => ({
        functionName: func.name,
        testName: test,
      })),
    );
    runBamlTests(testsToRun);
  };

  const handleToggleAll = () => {
    if (openCollapsibles.size === filteredFunctions.length) {
      // All are open, so collapse all
      setOpenCollapsibles(new Set());
    } else {
      // Some or none are open, so expand all
      const newOpenCollapsibles = new Set(
        filteredFunctions.map((func) => func.name),
      );
      setOpenCollapsibles(newOpenCollapsibles);
    }
  };

  const handleToggleCollapsible = (funcName: string) => {
    const newOpenCollapsibles = new Set(openCollapsibles);
    if (newOpenCollapsibles.has(funcName)) {
      newOpenCollapsibles.delete(funcName);
    } else {
      newOpenCollapsibles.add(funcName);
    }
    setOpenCollapsibles(newOpenCollapsibles);
  };

  const isAllExpanded =
    openCollapsibles.size === filteredFunctions.length &&
    filteredFunctions.length > 0;

  const maybe_mask = functionsAreStale ? 'pointer-events-none opacity-50' : '';

  return (
    <div className={cn('flex relative h-full', maybe_mask)}>
      <Sidebar
        variant="inset"
        collapsible="offcanvas"
        side="right"
        className="h-full border-l"
      >
        <SidebarHeader>
          <SidebarMenu>
            <SidebarMenuItem>
              <div className="flex items-center gap-2">
                <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg">
                  <FlaskConical className="size-4" />
                </div>
                <div className="flex flex-col gap-0.5 leading-none">
                  <span className="font-medium">Run BAML Tests</span>
                </div>
              </div>
            </SidebarMenuItem>
          </SidebarMenu>
          <SearchForm searchTerm={searchTerm} onSearchChange={setSearchTerm} />
        </SidebarHeader>
        <SidebarContent>
          <div className="flex-1 min-h-0 overflow-y-auto">
            {filteredFunctions.length > 0 && (
              <SidebarGroup>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      onClick={handleRunFilteredTests}
                      className="flex justify-between items-center w-full"
                    >
                      <span>Run all tests</span>
                      <Play className="w-3 h-3" />
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <SidebarMenuButton
                          onClick={handleToggleAll}
                          className="flex justify-between items-center py-1 w-full"
                          size="sm"
                        >
                          <span className="text-xs">
                            {isAllExpanded ? 'Collapse all' : 'Expand all'}
                          </span>
                          {isAllExpanded ? (
                            <ChevronUp className="w-3 h-3" />
                          ) : (
                            <ChevronDown className="w-3 h-3" />
                          )}
                        </SidebarMenuButton>
                      </TooltipTrigger>
                      <TooltipContent>
                        {isAllExpanded ? 'Collapse all' : 'Expand all'}
                      </TooltipContent>
                    </Tooltip>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroup>
            )}
            <SidebarGroup>
              <SidebarMenu>
                {filteredFunctions.map((func, index) => (
                  <Collapsible
                    key={func.name}
                    open={openCollapsibles.has(func.name)}
                    onOpenChange={() => handleToggleCollapsible(func.name)}
                    className="group/collapsible"
                  >
                    <SidebarMenuItem>
                      <SidebarMenuButton className="flex justify-between items-center w-full pl-8">
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <div className="flex items-center gap-2 truncate">
                              <FunctionSquare className="size-4" />
                              <span className="truncate">{func.name}</span>
                            </div>
                          </TooltipTrigger>
                          <TooltipContent>
                            <div className="flex items-center gap-2">
                              <FunctionSquare className="size-4" />
                              <span className="text-sm">{func.name}</span>
                            </div>
                          </TooltipContent>
                        </Tooltip>
                      </SidebarMenuButton>
                      {func.tests?.length > 0 && (
                        <>
                          <CollapsibleTrigger asChild>
                            <SidebarMenuAction className="bg-sidebar-accent text-sidebar-accent-foreground left-2 data-[state=open]:rotate-90">
                              <ChevronRight />
                            </SidebarMenuAction>
                          </CollapsibleTrigger>
                          <SidebarMenuAction
                            onClick={(e) => {
                              e.stopPropagation();
                              const testsToRun = func.tests.map((test) => ({
                                functionName: func.name,
                                testName: test,
                              }));
                              runBamlTests(testsToRun);
                            }}
                          >
                            <Play />
                          </SidebarMenuAction>
                        </>
                      )}
                      {func.tests?.length ? (
                        <CollapsibleContent>
                          <SidebarMenuSub className="pr-0 mr-0">
                            {func.tests.map((test) => (
                              <TestItem
                                key={test}
                                label={test}
                                isSelected={
                                  selectedItem?.[0] === func.name &&
                                  selectedItem?.[1] === test
                                }
                                searchTerm={searchTerm}
                                functionName={func.name}
                              />
                            ))}
                          </SidebarMenuSub>
                        </CollapsibleContent>
                      ) : null}
                    </SidebarMenuItem>
                  </Collapsible>
                ))}
              </SidebarMenu>
            </SidebarGroup>
          </div>
        </SidebarContent>
        <SidebarRail />
      </Sidebar>
    </div>
  );
}
