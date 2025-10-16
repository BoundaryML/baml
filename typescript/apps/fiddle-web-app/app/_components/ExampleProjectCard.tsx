'use client';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@baml/ui/alert-dialog';
import { Card, CardDescription, CardHeader, CardTitle } from '@baml/ui/card';
import { cn } from '@baml/ui/lib/utils';
import { useAtomValue } from 'jotai';
import { usePathname, useRouter } from 'next/navigation';
import type { BAMLProject } from '../../lib/exampleProjects';
import { unsavedChangesAtom } from '../[project_id]/_atoms/atoms';

export const ExampleProjectCard = ({ project }: { project: BAMLProject }) => {
  // const searchParams = useSearchParams()
  const router = useRouter();
  const selectedId = usePathname()?.replace('/', '');
  const isSelected =
    selectedId === project.id ||
    (project.id === 'extract-verbs' && selectedId === '');
  const unsavedChanges = useAtomValue(unsavedChangesAtom);
  return (
    <AlertDialog>
      <AlertDialogTrigger>
        <Card
          className={cn(
            'flex w-[200px] h-[140px] text-center px-2 font-sans border-zinc-700 border-[1px] bg-zinc-800/40 hover:cursor-pointer hover:bg-zinc-800 rounded-sm',
            [isSelected ? 'border-gray-600 bg-zinc-800' : ''],
          )}
          onClick={() => {
            // TODO use Link since the data can be prefetched
            if (!unsavedChanges) {
              router.push(`/${project.id}`);
              // router.refresh()
            }
          }}
        >
          <CardHeader className="w-full px-1 py-4">
            <CardTitle className="text-base text-left">
              {project.name}
            </CardTitle>
            <CardDescription className="text-sm text-left">
              {project.description}
            </CardDescription>
          </CardHeader>
        </Card>
      </AlertDialogTrigger>
      {unsavedChanges && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                router.push(`/${project.id}`, { scroll: false });
              }}
            >
              Yes, continue
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
};
