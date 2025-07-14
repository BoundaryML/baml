/* eslint-disable @typescript-eslint/no-floating-promises */
import { Input } from '@baml/ui/input';
import { cn } from '@baml/ui/lib/utils';
import { ScrollArea } from '@baml/ui/scroll-area';
import {
  Sidebar,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@baml/ui/sidebar';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import {
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  FlaskConical,
  Play,
  Search,
  XCircle,
} from 'lucide-react';
import { AnimatePresence, motion } from 'motion/react';
import * as React from 'react';
import { runtimeStateAtom, selectedItemAtom } from '../atoms';
import { Loader } from '../prompt-preview/components';
import {
  selectedHistoryIndexAtom,
  testHistoryAtom,
} from '../prompt-preview/test-panel/atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import { getStatus } from '../prompt-preview/test-panel/testStateUtils';

import { vscode } from '../../vscode'

interface FunctionData {
  name: string;
  tests: string[];
}

const functionsAtom = atom((get) => {
  const runtimeState = get(runtimeStateAtom);
  if (runtimeState === undefined) {
    return [];
  }
  return runtimeState.functions.map((f) => ({
    name: f.name,
    tests: f.test_cases.map((t) => t.name),
  }));
});

const functionsAreStaleAtom = atom((get) => {
  const runtimeState = get(runtimeStateAtom);
  return runtimeState.stale;
});

const isEmbed =
  typeof window !== 'undefined' && window.location.href.includes('embed');

export const isSidebarOpenAtom = atomWithStorage(
  'isSidebarOpen',
  isEmbed ? false : vscode.isVscode() ? true : false,
);

export default function CustomSidebar({
  isEmbed = false,
}: { isEmbed?: boolean }) {
  const functions = useAtomValue(functionsAtom);
  const rtState = useAtomValue(runtimeStateAtom);
  const [searchTerm, setSearchTerm] = React.useState('');
  const [isOpen, setIsOpen] = useAtom(isSidebarOpenAtom);
  const runBamlTests = useRunBamlTests();
  const functionsAreStale = useAtomValue(functionsAreStaleAtom);

  const filteredFunctions = functions.filter(
    (func) =>
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

  if (
    functions.length === 0 ||
    (functions.length === 1 && functions[0]?.tests.length === 1)
  ) {
    return <></>;
  }

  const maybe_mask = functionsAreStale ? 'pointer-events-none opacity-50' : '';

  return (
    <div className={cn('flex relative', maybe_mask)}>
      <Sidebar
        variant="inset"
        collapsible="offcanvas"
        className="border-r border-border bg-background/50"
        side="right"
      >
        <SidebarHeader className="h-[60px] px-4">
          <div className="relative w-full">
            <div className="absolute inset-0 -m-0.5 rounded-md transition-all" />
            <div className="flex relative items-center">
              <Search className="absolute left-2 top-1/2 w-3 h-3 text-gray-400 -translate-y-1/2" />
              <Input
                placeholder="Filter Tests"
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                className="flex px-8 py-2 w-full h-9 text-xs rounded-md border border-input bg-background focus:outline-hidden focus:ring-2 focus:ring-ring"
              />
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <ScrollArea className="flex w-full h-full" type="always">
            <SidebarMenu>
              {filteredFunctions.length > 0 && (
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={handleRunFilteredTests}
                    className="flex justify-between items-center w-full"
                  >
                    <span>Run tests below</span>
                    <Play className="ml-2 w-3 h-3" />
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )}
              {filteredFunctions.map((func) => (
                <FunctionItem
                  key={func.name}
                  label={func.name}
                  tests={func.tests}
                  searchTerm={searchTerm}
                />
              ))}
            </SidebarMenu>
          </ScrollArea>
        </SidebarContent>
      </Sidebar>
    </div>
  );
}

interface FunctionItemProps {
  label: string;
  tests: string[];
  searchTerm?: string;
}

function FunctionItem({ label, tests, searchTerm = '' }: FunctionItemProps) {
  const [isOpen, setIsOpen] = React.useState(true);
  const runBamlTests = useRunBamlTests();
  const setSelectedItem = useSetAtom(selectedItemAtom);
  const selectedItem = useAtomValue(selectedItemAtom);

  const handleRunAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    const testsToRun = tests.map((test) => ({
      functionName: label,
      testName: test,
    }));
    runBamlTests(testsToRun);
  };

  const highlightText = (text: string) => {
    if (!searchTerm) return text;
    const parts = text.split(new RegExp(`(${searchTerm})`, 'gi'));
    return (
      <span>
        {parts.map((part) =>
          part.toLowerCase() === searchTerm.toLowerCase() ? (
            <span key={part} className="bg-yellow-200 dark:bg-yellow-900">
              {part}
            </span>
          ) : (
            part
          ),
        )}
      </span>
    );
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => setIsOpen(!isOpen)}
        isActive={selectedItem?.[0] === label}
        className="flex justify-between items-center w-full"
      >
        <div className="flex items-center min-w-0">
          <motion.div
            initial={false}
            animate={{ rotate: isOpen ? 90 : 0 }}
            transition={{ duration: 0.2 }}
            className="mr-1"
          >
            <ChevronRight className="w-3 h-3" />
          </motion.div>
          <span className="ml-1 font-mono text-xs py-1 truncate">
            {highlightText(label)}
          </span>
        </div>
        <SidebarMenuAction
          onClick={handleRunAll}
          className="hidden group-hover:flex"
        >
          <Play className="w-3 h-3" />
        </SidebarMenuAction>
      </SidebarMenuButton>
      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden ml-4"
          >
            <SidebarMenu>
              {tests.map((test) => (
                <TestItem
                  key={test}
                  label={test}
                  isSelected={
                    selectedItem?.[0] === label && selectedItem?.[1] === test
                  }
                  searchTerm={searchTerm}
                  functionName={label}
                />
              ))}
            </SidebarMenu>
          </motion.div>
        )}
      </AnimatePresence>
    </SidebarMenuItem>
  );
}

interface TestItemProps {
  label: string;
  isSelected?: boolean;
  searchTerm?: string;
  functionName: string;
}

function TestItem({
  label,
  isSelected = false,
  searchTerm = '',
  functionName,
}: TestItemProps) {
  const testHistory = useAtomValue(testHistoryAtom);
  const selectedIndex = useAtomValue(selectedHistoryIndexAtom);
  const runBamlTests = useRunBamlTests();
  const setSelectedItem = useSetAtom(selectedItemAtom);

  const currentRun = testHistory[selectedIndex];
  const testResult = currentRun?.tests.find(
    (t) => t.functionName === functionName && t.testName === label,
  );

  const getStatusIcon = () => {
    if (!testResult) return <FlaskConical className="w-3 h-3" />;
    const status = testResult.response.status;
    const finalState = getStatus(testResult.response);
    if (status === 'running') return <Loader className="w-3 h-3" />;
    if (status === 'error') return <XCircle className="w-3 h-3 text-red-500" />;
    if (status === 'done') {
      if (finalState === 'passed')
        return <CheckCircle2 className="w-3 h-3 text-green-500" />;
      if (finalState === 'constraints_failed')
        return <AlertTriangle className="w-3 h-3 text-yellow-500" />;
      return <XCircle className="w-3 h-3 text-red-500" />;
    }
    return <FlaskConical className="w-3 h-3" />;
  };

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedItem(functionName, label);
  };

  const handleRunTest = (e: React.MouseEvent) => {
    e.stopPropagation();
    runBamlTests([{ functionName, testName: label }]);
  };

  const highlightText = (text: string) => {
    if (!searchTerm) return text;
    const parts = text.split(new RegExp(`(${searchTerm})`, 'gi'));
    return (
      <span>
        {parts.map((part) =>
          part.toLowerCase() === searchTerm.toLowerCase() ? (
            <span key={part} className="bg-yellow-200 dark:bg-yellow-900">
              {part}
            </span>
          ) : (
            part
          ),
        )}
      </span>
    );
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={handleClick}
        isActive={isSelected}
        className="flex justify-between items-center w-full"
      >
        <div className="flex items-center min-w-0">
          {getStatusIcon()}
          <span className="ml-1 font-mono text-[11px] group-hover:truncate min-w-[90px] group-hover:max-w-[90px] max-w-[95px]">
            {highlightText(label)}
          </span>
        </div>
        <SidebarMenuAction
          onClick={handleRunTest}
          className="opacity-0 transition-opacity group-hover:opacity-100"
        >
          <Play className="w-3 h-3" />
        </SidebarMenuAction>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
