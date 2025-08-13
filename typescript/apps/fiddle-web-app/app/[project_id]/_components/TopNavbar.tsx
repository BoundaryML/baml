'use client';
/* eslint-disable @typescript-eslint/no-misused-promises */

import { Loader } from '@baml/playground-common';
import { Badge } from '@baml/ui/badge';
import { Button } from '@baml/ui/button';
import { toast } from '@baml/ui/sonner';
import { useAtom } from 'jotai';
import { useAtomValue } from 'jotai';
import {
  AlertTriangleIcon,
  Code,
  Compass,
  File,
  GitForkIcon,
  LinkIcon,
} from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import posthog from 'posthog-js';
import { useState } from 'react';
import { SiDiscord } from 'react-icons/si';
import { createUrl } from '../../../app/actions';
import type { BAMLProject } from '../../../lib/exampleProjects';
import { Editable } from '../../_components/EditableText';
import { unsavedChangesAtom } from '../_atoms/atoms';
import { currentEditorFilesAtom } from '../_atoms/atoms';
import { EmbedDialog } from './EmbedDialog';
import { GithubStars } from './GithubStars';

const ShareButton = ({
  project,
  projectName,
}: { project: BAMLProject; projectName: string }) => {
  const [loading, setLoading] = useState(false);
  const editorFiles = useAtomValue(currentEditorFilesAtom);
  const pathname = usePathname();
  const [unsavedChanges, setUnsavedChanges] = useAtom(unsavedChangesAtom);

  return (
    <Button
      variant={'default'}
      className="gap-x-2 py-1 h-full whitespace-nowrap bg-transparent text-secondary-foreground hover:bg-purple-600 w-fit disabled:opacity-50"
      disabled={loading}
      onClick={async () => {
        setLoading(true);
        try {
          if (typeof window === 'undefined') {
            return;
          }
          let urlId = pathname?.split('/')[1];
          // if (!urlId || urlId === 'new-project') {
          //   console.log('creating url')
          urlId = await createUrl({
            ...project,
            name: projectName,
            files: editorFiles,
            // TODO: @hellovai use runTestOutput
            testRunOutput: undefined,
          });

          posthog.capture('share_url', { id: urlId });

          const newUrl = `${window?.location.origin}/${urlId}`;
          window.history.replaceState(
            { ...window.history.state, as: newUrl, url: newUrl },
            '',
            newUrl,
          );
          setUnsavedChanges(false);
          // }

          navigator.clipboard.writeText(`${window?.location.origin}/${urlId}`);

          toast.success('URL copied to clipboard', {
            description: `${window?.location.origin}/${urlId}`,
          });
        } catch (e) {
          posthog.capture('share_url_failed', { error: JSON.stringify(e) });
          toast.error('Failed to generate URL', {
            description: 'Please try again',
          });
          console.error(e);
        } finally {
          setLoading(false);
        }
      }}
    >
      {unsavedChanges ? <GitForkIcon size={14} /> : <LinkIcon size={14} />}
      <span>{unsavedChanges ? 'Fork & Share' : 'Share'}</span>
      {loading && <Loader />}
    </Button>
  );
};

interface ProjectHeaderProps {
  project: BAMLProject;
  projectName: string;
  setProjectName: (name: string) => void;
  projectNameInputRef: React.RefObject<HTMLInputElement | null>;
  unsavedChanges: boolean;
}

export const TopNavbar = ({
  project,
  projectName,
  setProjectName,
  projectNameInputRef,
  unsavedChanges,
}: ProjectHeaderProps) => {
  const [embedDialogOpen, setEmbedDialogOpen] = useState(false);

  return (
    <div className="flex flex-row items-center gap-x-12 border-b-[0px] min-h-[55px]">
      <div className="flex flex-col items-center py-1 h-full whitespace-nowrap lg:pr-4 tour-title w-[200px] ">
        <Editable
          text={projectName}
          placeholder="Write a task name"
          type="input"
          childRef={projectNameInputRef}
        >
          <input
            className="px-1 text-lg border-none text-foreground w-[140px]"
            type="text"
            ref={projectNameInputRef}
            name="task"
            placeholder="Write a task name"
            value={projectName}
            onChange={(e) => setProjectName(e.target.value)}
          />
        </Editable>
      </div>

      <div className="flex flex-row gap-x-2 items-center">
        <ShareButton project={project} projectName={projectName} />
      </div>

      <div className="flex items-center justify-start h-full pt-0.5 ">
        <Button
          asChild
          variant={'ghost'}
          className="gap-x-1 py-1 h-full hover:bg-purple-600"
        >
          <Link
            href="https://boundaryml.com"
            target="_blank"
            className="text-sm hover:text-foreground text-foreground/60"
          >
            What is BAML?
          </Link>
        </Button>
      </div>

      {project.id !== 'all-projects' && project.id !== null ? (
        <div className="flex flex-col justify-center items-center h-full">
          <Link
            href="/all-projects"
            target="_blank"
            className="flex flex-row gap-x-2 items-center px-2 py-1 text-sm text-white whitespace-pre-wrap bg-purple-500 rounded-sm dark:bg-purple-600 hover:bg-purple-300 dark:hover:bg-purple-700 h-fit"
          >
            <Compass size={16} strokeWidth={2} />
            <span className="whitespace-nowrap">Explore Examples</span>
          </Link>
        </div>
      ) : null}

      <div className="flex flex-col justify-center items-center h-full">
        <Link
          href="/new-project"
          target="_blank"
          className="flex flex-row gap-x-2 items-center px-2 py-1 text-sm text-white whitespace-pre-wrap bg-purple-500 rounded-sm dark:bg-purple-600 hover:bg-purple-600 dark:hover:bg-purple-700 h-fit"
        >
          <File size={16} strokeWidth={2} />
          <span className="whitespace-nowrap">New project</span>
        </Link>
      </div>

      <div className="flex flex-col justify-center items-center h-full">
        <Button
          onClick={() => setEmbedDialogOpen(true)}
          className="flex flex-row gap-x-2 items-center px-2 py-1 text-sm text-white whitespace-pre-wrap bg-blue-500 rounded-sm dark:bg-blue-600 hover:bg-blue-300 dark:hover:bg-blue-700 h-fit"
        >
          <Code size={16} strokeWidth={2} />
          <span className="whitespace-nowrap">Embed</span>
        </Button>
      </div>

      <EmbedDialog
        open={embedDialogOpen}
        onOpenChange={setEmbedDialogOpen}
        shareId={project.id}
        project={project}
        projectName={projectName}
      />

      {unsavedChanges ? (
        <div className="flex flex-row items-center whitespace-nowrap text-muted-foreground">
          <Badge
            variant="outline"
            className="gap-x-2 font-light text-yellow-600 dark:text-yellow-500"
          >
            <AlertTriangleIcon size={14} />
            <span>Unsaved changes</span>
          </Badge>
        </div>
      ) : (
        <></>
      )}

      <div className="flex flex-row gap-x-8 justify-end items-center pr-4 w-full">
        <div className="flex h-full">
          <Link
            href="https://discord.gg/BTNBeXGuaS"
            className="pt-0 h-full w-fit"
          >
            <div className="flex flex-row gap-x-4 items-center text-sm">
              <SiDiscord size={24} className="opacity-40 hover:opacity-100" />
              {/* <Image
                src="/discord-icon.svg"
                className="text-blue-600 fill-black hover:opacity-100"
                width={24}
                height={24}
                alt="Discord"
              /> */}
            </div>
          </Link>
        </div>
        <div className="flex h-full">
          <Link
            href="https://docs.boundaryml.com/guide/installation-editors/vs-code-extension"
            className="pt-0 h-full w-fit text-zinc-400 dark:text-zinc-300 dark:hover:text-zinc-50 hover:text-zinc-600"
          >
            <div className="flex flex-row gap-x-4 items-center text-xs grayscale 2xl:text-sm hover:grayscale-0">
              <Image
                src="/vscode_logo.svg"
                width={18}
                height={18}
                alt="VSCode extension"
              />
            </div>
          </Link>
        </div>
        <div className="flex h-full">
          <GithubStars />
        </div>
      </div>
    </div>
  );
};
