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
  SidebarTrigger,
  useSidebar,
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
  Square,
} from 'lucide-react';
import * as React from 'react';
import { areTestsRunningAtom } from '../atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import { testHistoryAtom, selectedHistoryIndexAtom } from '../prompt-preview/test-panel/atoms';
import { functionsAtom, isSidebarOpenAtom } from './atoms';
import { FunctionItem } from './function-item';
import { SearchForm } from './search-form';
import { TestItem } from './test-item';
import type { FunctionData } from './types';
import { Button } from '@baml/ui/button';
import { unifiedSelectionStateAtom } from '../../../../sdk/atoms/core.atoms';

export { isSidebarOpenAtom };

export function TestingSidebar() {
  const functions = useAtomValue(functionsAtom);
  const [searchTerm, setSearchTerm] = React.useState('');
  const [openCollapsibles, setOpenCollapsibles] = React.useState<Set<string>>(
    new Set(),
  );
  const { runTests: runBamlTests, cancelTests } = useRunBamlTests();
  const selection = useAtomValue(unifiedSelectionStateAtom);
  const areTestsRunning = useAtomValue(areTestsRunningAtom);
  const testHistory = useAtomValue(testHistoryAtom);
  const selectedIndex = useAtomValue(selectedHistoryIndexAtom);

  const currentRun = testHistory[selectedIndex];


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

  // Auto-expand function when its test is selected
  React.useEffect(() => {
    const functionName = selection.mode === 'function' || selection.mode === 'workflow'
      ? selection.functionName
      : null;
    const testName = selection.mode === 'function' || selection.mode === 'workflow'
      ? selection.testName
      : null;

    if (functionName && testName) {
      // Check if this function exists and has this test
      const func = functions.find(f => f.name === functionName);
      if (func && func.tests.includes(testName) && !openCollapsibles.has(functionName)) {
        setOpenCollapsibles(prev => new Set([...prev, functionName]));
      }
    }
  }, [selection, functions, openCollapsibles]);

  return (
    // <div className={cn('flex relative h-full')}>
    <Sidebar variant="inset" collapsible="offcanvas" side="right">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div className="flex items-center gap-1.5 relative">
              <div className="text-sidebar-primary-foreground flex aspect-square size-5 items-center justify-center rounded-lg">
                <FlaskConical className="size-3" />
              </div>
              <div className="flex flex-col gap-0.5 leading-none">
                <span className="font-semibold text-[10px] uppercase tracking-wide">BAML Tests</span>
              </div>
            </div>
            <div className='absolute right-0 top-0 xl:hidden'>
              <SidebarTrigger />
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
            <SidebarGroup className="pl-0">
              <SidebarMenu className="gap-0 pl-0">
                <SidebarMenuItem className="pl-0">
                  <SidebarMenuButton
                    onClick={() => {
                      if (areTestsRunning) {
                        cancelTests()
                      } else {
                        handleRunFilteredTests()
                      }
                    }}
                    className="flex  w-full cursor-pointer text-[10px] px-2 py-1 h-6 items-center justify-center pt-0.5"
                  >
                    <span>{areTestsRunning ? 'Stop tests' : 'Run all tests'}</span>
                    {areTestsRunning ? (
                      <Square className="!size-3 fill-red-500 stroke-red-500" />
                    ) : (
                      <Play className="!size-3" />
                    )}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </SidebarGroup>
          )}
          {filteredFunctions.length > 0 && <SidebarSeparator />}
          <SidebarGroup className="pl-0">
            <SidebarMenu className="gap-0.5">
              {filteredFunctions.length > 0 && (
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={handleToggleAll}
                    className="flex justify-between items-center py-0.5 w-full text-[9px] px-2 h-5"
                    size="sm"
                  >
                    <span>
                      {isAllExpanded ? 'Collapse all' : 'Expand all'}
                    </span>
                    {isAllExpanded ? (
                      <ChevronUp className="w-2.5 h-2.5" />
                    ) : (
                      <ChevronDown className="w-2.5 h-2.5" />
                    )}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )}
              {filteredFunctions.map((func) => {
                // Check if any of this function's tests are running
                const isFunctionRunning = currentRun?.tests.some(
                  (test) => test.functionName === func.name && test.response.status === 'running'
                ) ?? false;

                return (
                  <Collapsible
                    key={func.name}
                    open={openCollapsibles.has(func.name)}
                    onOpenChange={() => handleToggleCollapsible(func.name)}
                    className="group/collapsible"
                  >
                    <SidebarMenuItem>
                      <FunctionItem
                        functionName={func.name}
                        tests={func.tests}
                        functionFlavor={func.functionFlavor}
                        isSelected={
                          (selection.mode === 'workflow' && selection.selectedNodeId === func.name) ||
                          (selection.mode === 'function' && selection.functionName === func.name)
                        }
                        onToggle={() => {
                          // Toggle expansion when clicked
                          handleToggleCollapsible(func.name);
                        }}
                      />
                      {func.tests?.length > 0 && (
                        <>
                          <CollapsibleTrigger asChild>
                            <SidebarMenuAction className="bg-sidebar-accent items-center size-3 text-sidebar-accent-foreground left-2 top-0.5 data-[state=open]:rotate-90 cursor-pointer">
                              <ChevronRight className="size-1" />
                            </SidebarMenuAction>
                          </CollapsibleTrigger>
                          <SidebarMenuAction
                            className="cursor-pointer size-[8px] items-center justify-center pt-0.5"
                            onClick={(e) => {
                              e.stopPropagation();
                              if (isFunctionRunning) {
                                cancelTests();
                              } else {
                                const testsToRun = func.tests.map((test) => ({
                                  functionName: func.name,
                                  testName: test,
                                }));
                                runBamlTests(testsToRun);
                              }
                            }}
                          >
                            {isFunctionRunning ? (
                              <Square className="size-2.5 fill-red-500 stroke-red-500" />
                            ) : (
                              <Play className="!size-3" />
                            )}
                          </SidebarMenuAction>
                        </>
                      )}
                      {func.tests?.length ? (
                        <CollapsibleContent>
                          <SidebarMenuSub className="pl-6 pr-0 mr-0 space-y-0">
                            {func.tests.map((test) => (
                              <TestItem
                                key={test}
                                label={test}
                                isSelected={
                                  ((selection.mode === 'function' && selection.functionName === func.name && selection.testName === test) ||
                                   (selection.mode === 'workflow' && selection.functionName === func.name && selection.testName === test))
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
                );
              })}
            </SidebarMenu>
          </SidebarGroup>
        </div>
      </SidebarContent>
      <SidebarRail />
    </Sidebar>
  );
}
