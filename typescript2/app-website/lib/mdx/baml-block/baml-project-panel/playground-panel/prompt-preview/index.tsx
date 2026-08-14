'use client';

import {
  ResizableHandle,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ResizablePanel } from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAtomValue } from 'jotai';
import { useEffect, useRef } from 'react';
import { type ImperativePanelHandle } from 'react-resizable-panels';
import { areTestsRunningAtom } from '../atoms';
import PreviewToolbar from '../preview-toolbar';
import SideBar from '../side-bar';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import TestPanel from './test-panel';

const PromptPreview = () => {
  const areTestsRunning = useAtomValue(areTestsRunningAtom);
  const ref = useRef<ImperativePanelHandle>(null);

  const handleResize = () => {
    if (ref.current) {
      if (areTestsRunning) {
        // expand the test panel to 70% of the height
        console.log('ref.current.getSize()', ref.current.getSize());
        if (ref.current.getSize() < 30) {
          console.log('resizing to 70');
          ref.current.resize(70);
        }
      } else {
        // ref.current.resize(20);
      }
    }
  };

  useEffect(() => {
    handleResize();
  }, [areTestsRunning]);

  return (
    <div className="relative flex h-full justify-between text-xs">
      <div
        className="flex h-full w-full flex-col items-start justify-start overflow-x-auto"
        style={{ minHeight: '530px' }}
      >
        <ResizablePanelGroup
          autoSaveId={'prompt-preview'}
          direction="vertical"
          className="h-full py-2"
        >
          <ResizablePanel
            defaultSize={areTestsRunning ? 40 : 80}
            className="flex flex-col gap-4 px-4"
          >
            <PreviewToolbar />
            <ScrollArea className="h-full w-full" type="always">
              <PromptRenderWrapper />
            </ScrollArea>
          </ResizablePanel>
          <ResizableHandle withHandle />
          <ResizablePanel
            ref={ref}
            defaultSize={areTestsRunning ? 60 : 20}
            className="px-2"
          >
            <ScrollArea className="h-full w-full" type="always">
              <TestPanel />
            </ScrollArea>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
      <SideBar />
    </div>
  );
};

export default PromptPreview;
