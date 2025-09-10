'use client';

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@baml/ui/collapsible';
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
  SidebarSeparator,
} from '@baml/ui/sidebar';
import { useAtomValue } from 'jotai';
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  FlaskConical,
  Play,
  Settings,
  SidebarCloseIcon,
} from 'lucide-react';
import * as React from 'react';
import { selectedItemAtom } from '../atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import { functionsAtom, isSidebarOpenAtom } from './atoms';
import { FunctionItem } from './function-item';
import { SearchForm } from './search-form';
import { TestItem } from './test-item';
import type { FunctionData } from './types';
import { Button } from '@baml/ui/button';

export { isSidebarOpenAtom };

export function TestingSidebar() {
  const functions = useAtomValue(functionsAtom);
  const [searchTerm, setSearchTerm] = React.useState('');
  const [openCollapsibles, setOpenCollapsibles] = React.useState<Set<string>>(
    new Set(),
  );
  const { runTests: runBamlTests } = useRunBamlTests();
  const selectedItem = useAtomValue(selectedItemAtom);

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

  return (
    // <div className={cn('flex relative h-full')}>
    <Sidebar variant="inset" collapsible="offcanvas" side="right">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div className="flex items-center gap-2">
              <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg">
                <FlaskConical className="size-4" />
              </div>
              <div className="flex flex-col gap-0.5 leading-none">
                <span className="font-medium">BAML Tests</span>
              </div>
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
        <SearchForm searchTerm={searchTerm} onSearchChange={setSearchTerm} />
        {/* <Button variant="ghost" size="sm" className='absolute right-0 top-0 xl:hidden' onClick={() => {
          
        }}>
          <SidebarCloseIcon className="size-4 scale-x-[-1]" />
        </Button> */}
      </SidebarHeader>
      <SidebarContent>
        <div className="flex-1 min-h-0 overflow-y-auto">
          {filteredFunctions.length === 0 && (
            <div className="flex flex-col items-center justify-center mt-4">
              <span className="text-muted-foreground">No functions found</span>
            </div>
          )}
          {filteredFunctions.length > 0 && (
            <SidebarGroup>
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={handleRunFilteredTests}
                    className="flex justify-between items-center w-full cursor-pointer"
                  >
                    <span>Run all tests</span>
                    <Play className="w-3 h-3" />
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroup>
          )}
          {filteredFunctions.length > 0 && <SidebarSeparator />}
          <SidebarGroup>
            <SidebarMenu>
              {filteredFunctions.length > 0 && (
                <SidebarMenuItem>
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
                </SidebarMenuItem>
              )}
              {filteredFunctions.map((func) => (
                <Collapsible
                  key={func.name}
                  open={openCollapsibles.has(func.name)}
                  onOpenChange={() => handleToggleCollapsible(func.name)}
                  className="group/collapsible"
                >
                  <SidebarMenuItem>
                    <FunctionItem functionName={func.name} tests={func.tests} />
                    {func.tests?.length > 0 && (
                      <>
                        <CollapsibleTrigger asChild>
                          <SidebarMenuAction className="bg-sidebar-accent text-sidebar-accent-foreground left-2 data-[state=open]:rotate-90 cursor-pointer">
                            <ChevronRight />
                          </SidebarMenuAction>
                        </CollapsibleTrigger>
                        <SidebarMenuAction
                          className="cursor-pointer"
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
                        <SidebarMenuSub className="pl-8 pr-0 mr-0">
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
  );
}
