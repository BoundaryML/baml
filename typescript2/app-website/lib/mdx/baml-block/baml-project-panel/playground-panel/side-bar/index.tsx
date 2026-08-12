import { Input } from '@/components/ui/input';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { ResizablePanel } from '@/components/ui/resizable';
import { ResizablePanelGroup } from '@/components/ui/resizable';
import { cn } from '@/lib/utils';
import { Dialog } from '@radix-ui/react-dialog';
import { DialogContent } from '@radix-ui/react-dialog';
import { AnimatePresence, motion } from 'framer-motion';
import { atom, useAtomValue, useSetAtom } from 'jotai';
import {
  ChevronLeft,
  ChevronRight,
  FlaskConical,
  Search,
  Settings,
} from 'lucide-react';
import * as React from 'react';
import { Button } from '@/components/ui/button';
import { runtimeStateAtom, selectedItemAtom } from '../atoms';
import EnvVars from './env-vars';

interface FunctionData {
  name: string;
  tests: string[];
}

interface SidepanelProps {
  functions?: FunctionData[];
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

export default function CustomSidebar() {
  const functions = useAtomValue(functionsAtom);
  const [searchTerm, setSearchTerm] = React.useState('');
  const [showEnvDialog, setShowEnvDialog] = React.useState(false);
  const [isOpen, setIsOpen] = React.useState(true);

  const filteredFunctions = functions.filter(
    (func) =>
      func.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      func.tests.some((test) =>
        test.toLowerCase().includes(searchTerm.toLowerCase()),
      ),
  );

  if (
    functions.length === 0 ||
    (functions.length === 1 && functions[0]?.tests.length === 1)
  ) {
    return (
      <>
        <Button
          variant="ghost"
          size="sm"
          className="absolute right-1 top-0 flex items-center gap-2 text-muted-foreground/60"
          onClick={() => setShowEnvDialog(true)}
        >
          <Settings className="h-4 w-4" />
          <span>API Keys</span>
        </Button>

        <Dialog open={showEnvDialog} onOpenChange={setShowEnvDialog}>
          <DialogContent className="mt-12 sm:max-w-[625px]">
            <EnvVars />
          </DialogContent>
        </Dialog>
      </>
    );
  }

  return (
    <div className="relative flex h-full">
      <Button
        onClick={() => setIsOpen(!isOpen)}
        variant="ghost"
        size="sm"
        className={cn(
          'absolute -left-6 top-1/2 z-10 h-12 w-8 -translate-y-1/2 p-0 hover:bg-muted',
          isOpen ? 'rounded-l' : 'rounded',
        )}
      >
        <ChevronLeft
          className={cn(
            'h-4 w-4 transition-transform duration-200',
            isOpen ? 'rotate-180' : '',
          )}
        />
        <span className="sr-only">Toggle sidebar</span>
      </Button>

      <Popover open={showEnvDialog} onOpenChange={setShowEnvDialog}>
        <PopoverTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="absolute right-1 bottom-0 flex items-center gap-2 text-muted-foreground/60"
            onClick={() => setShowEnvDialog(true)}
          >
            <Settings className="h-4 w-4" />
            <span>API Keys</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent className="sm:max-w-[625px] max-h-[400px] border-[1px] border-gray-200 h-full">
          <EnvVars />
        </PopoverContent>
      </Popover>

      <div
        className={cn(
          'flex h-full flex-col border-l bg-muted/20 transition-all duration-200',
          isOpen
            ? 'w-[160px] min-w-[160px] opacity-100'
            : 'w-8 min-w-8 opacity-100',
        )}
      >
        <ResizablePanelGroup direction="vertical">
          <ResizablePanel defaultSize={75}>
            <div className="flex h-[60px] items-center px-4 text-xs">
              <div className="relative w-full">
                <div className="absolute inset-0 -m-0.5 rounded-md transition-all" />
                <div className="relative flex items-center">
                  <Search className="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-gray-400" />
                  <Input
                    placeholder="Filter..."
                    value={searchTerm}
                    onChange={(e) => setSearchTerm(e.target.value)}
                    className="flex h-9 w-full rounded-md border border-gray-300 bg-white px-8 py-2 text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
              </div>
            </div>
            <div className="flex-1 overflow-auto">
              <div className="px-2">
                <TreeView data={filteredFunctions} searchTerm={searchTerm} />
              </div>
            </div>
          </ResizablePanel>
          {/* <ResizableHandle withHandle />
          <ResizablePanel defaultSize={25}>
            <ScrollArea className="h-full" type="always">
              <EnvVars />
            </ScrollArea>
          </ResizablePanel> */}
        </ResizablePanelGroup>
      </div>
    </div>
  );
}

interface FunctionItemProps {
  label: string;
  children: React.ReactNode;
  isLast?: boolean;
  isSelected?: boolean;
  searchTerm?: string;
}

function FunctionItem({
  label,
  children,
  isLast = false,
  isSelected = false,
  searchTerm = '',
}: FunctionItemProps) {
  const [isOpen, setIsOpen] = React.useState(true);

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsOpen(!isOpen);
  };

  const highlightText = (text: string) => {
    if (!searchTerm) return text;

    const parts = text.split(new RegExp(`(${searchTerm})`, 'gi'));
    return (
      <span>
        {parts.map((part, i) =>
          part.toLowerCase() === searchTerm.toLowerCase() ? (
            <span key={i} className="bg-yellow-200">
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
    <motion.div
      initial={{ opacity: 0, y: -10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.2 }}
    >
      <div
        className={`relative -mx-2 flex cursor-pointer items-center px-1 py-1 transition-colors hover:bg-gray-100 ${
          isSelected ? 'font-bold text-purple-800' : 'text-muted-foreground'
        }`}
        onClick={handleClick}
      >
        <div className="flex min-w-0 flex-1 items-center">
          <motion.div
            initial={false}
            animate={{ rotate: isOpen ? 90 : 0 }}
            transition={{ duration: 0.2 }}
            className="mr-1"
          >
            <ChevronRight className="h-3 w-3" />
          </motion.div>
          <span className="ml-1 truncate font-mono">
            {highlightText(label)}
          </span>
        </div>
      </div>
      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2 }}
            className="ml-4 overflow-hidden"
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

interface TestItemProps {
  label: string;
  isLast?: boolean;
  isSelected?: boolean;
  searchTerm?: string;
  functionName: string;
}

function TestItem({
  label,
  isLast = false,
  isSelected = false,
  searchTerm = '',
  functionName,
}: TestItemProps) {
  const onSelect = useSetAtom(selectedItemAtom);

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect(functionName, label);
  };

  const highlightText = (text: string) => {
    if (!searchTerm) return text;

    const parts = text.split(new RegExp(`(${searchTerm})`, 'gi'));
    return (
      <span>
        {parts.map((part, i) =>
          part.toLowerCase() === searchTerm.toLowerCase() ? (
            <span key={i} className="bg-yellow-200">
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
    <motion.div
      initial={{ opacity: 0, y: -10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.2 }}
      className="ml-2"
    >
      <div
        className={`relative -mx-2 flex cursor-pointer items-center px-1 py-1 transition-colors ${
          isSelected
            ? 'border-l-4 border-l-purple-500 bg-purple-50'
            : 'hover:bg-gray-100'
        } ${isSelected ? 'text-foreground' : 'text-muted-foreground'}`}
        onClick={handleClick}
      >
        <div className="flex min-w-0 flex-1 items-center">
          <FlaskConical className="h-3 w-3" />
          <span className="ml-1 truncate font-mono">
            {highlightText(label)}
          </span>
        </div>
      </div>
    </motion.div>
  );
}

function TreeView({
  data,
  searchTerm,
}: {
  data: FunctionData[];
  searchTerm: string;
}) {
  const selectedItem = useAtomValue(selectedItemAtom);

  return (
    <div className="space-y-1">
      {data.map((func, index) => (
        <FunctionItem
          key={index}
          label={func.name}
          isLast={index === data.length - 1}
          isSelected={selectedItem?.[0] === func.name}
          searchTerm={searchTerm}
        >
          {func.tests.map((test, testIndex) => (
            <TestItem
              key={testIndex}
              label={test}
              isLast={testIndex === func.tests.length - 1}
              isSelected={
                selectedItem?.[0] === func.name && selectedItem?.[1] === test
              }
              searchTerm={searchTerm}
              functionName={func.name}
            />
          ))}
        </FunctionItem>
      ))}
    </div>
  );
}
