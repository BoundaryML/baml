'use client';
import { SidebarInset, SidebarProvider } from '@baml/ui/sidebar';
import { useAtom } from 'jotai';
import { showEnvDialogAtom } from '../atoms';
import { PreviewToolbar } from '../preview-toolbar';
import { isSidebarOpenAtom } from '../side-bar';
import { EnvironmentVariablesDialog } from '../side-bar/env-vars';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import { TestPanel } from './test-panel';

export const PromptPreview = ({ isEmbed = false }: { isEmbed?: boolean }) => {
  const [showEnvDialog, setShowEnvDialog] = useAtom(showEnvDialogAtom);
  const [isOpen, setIsOpen] = useAtom(isSidebarOpenAtom);

  return (
    <SidebarProvider>
      <SidebarInset>
        <div className="flex w-full h-full bg-background text-foreground">
          <div className="flex overflow-y-auto flex-col w-full h-full gap-2">
            <EnvironmentVariablesDialog
              showEnvDialog={showEnvDialog}
              setShowEnvDialog={setShowEnvDialog}
            />
            <PreviewToolbar />
            <PromptRenderWrapper />
            <TestPanel />
          </div>
        </div>
      </SidebarInset>
      {/* <SideBar /> */}
    </SidebarProvider>
  );
};
