'use client';

import { Alert, AlertDescription, AlertTitle } from '@baml/ui/alert';
import { CopyButton } from '@baml/ui/custom/copy-button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@baml/ui/dialog';
import { Label } from '@baml/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@baml/ui/tabs';
import { Code, Info, Link as LinkIcon } from 'lucide-react';
import dynamic from 'next/dynamic';
import { useState, useEffect } from 'react';
import { SiReact } from 'react-icons/si';
import { useAtomValue } from 'jotai';
import { currentEditorFilesAtom } from '../_atoms/atoms';
import { createUrl } from '../../../app/actions';
import type { BAMLProject } from '../../../lib/exampleProjects';
import { usePathname } from 'next/navigation';
import { Switch } from '@baml/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@baml/ui/select';

const ProjectView = dynamic(() => import('./ProjectView'), { ssr: false });

interface EmbedDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  shareId?: string;
  project: BAMLProject;
  projectName: string;
}

export function EmbedDialog({
  open,
  onOpenChange,
  shareId,
  project,
  projectName,
}: EmbedDialogProps) {
  const [activeTab, setActiveTab] = useState('link');
  const [generatedUrl, setGeneratedUrl] = useState('');
  const editorFiles = useAtomValue(currentEditorFilesAtom);
  const pathname = usePathname();
  const [showFileTree, setShowFileTree] = useState<boolean>(false);
  const [showPlayground, setShowPlayground] = useState<boolean>(true);
  const [defaultFile, setDefaultFile] = useState<string>('');
  const [showFile, setShowFile] = useState<boolean>(true);

  const buildUrl = (origin: string, id: string) => {
    const url = new URL(`${origin}/embed`);
    url.searchParams.set('id', id);
    if (showFileTree) url.searchParams.set('showFileTree', 'true');
    if (!showPlayground) url.searchParams.set('showPlayground', 'false');
    if (!showFile) url.searchParams.set('showFile', 'false');
    if (defaultFile.trim()) url.searchParams.set('defaultFile', defaultFile.trim());
    return url.toString();
  };

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    (async () => {
      try {
        if (typeof window === 'undefined') return;

        // Prefer existing id from URL, otherwise create a new one like the Share button
        let urlId = pathname?.split('/')[1];
        if (!urlId || urlId === 'new-project') {
          urlId = await createUrl({
            ...project,
            name: projectName,
            files: editorFiles,
          } as BAMLProject);
        }

        if (!cancelled) {
          setGeneratedUrl(buildUrl(window.location.origin, urlId));
        }
      } catch (e) {
        // Fallback to provided shareId if creation fails
        if (!cancelled) {
          if (shareId && typeof window !== 'undefined') {
            setGeneratedUrl(buildUrl(window.location.origin, shareId));
          } else {
            setGeneratedUrl('');
          }
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, project, projectName, editorFiles, shareId, pathname, showFileTree, showPlayground, defaultFile, showFile]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Code className="h-5 w-5" />
            Create Embed Link
          </DialogTitle>
          <DialogDescription>
            Generate embeddable links for your BAML functions to share in the
            playground
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 p-3 rounded-md border">
            <div className="flex items-center justify-between gap-3">
              <Label htmlFor="showFileTree">Show file tree</Label>
              <Switch id="showFileTree" checked={showFileTree} onCheckedChange={(v) => setShowFileTree(!!v)} />
            </div>
            <div className="flex items-center justify-between gap-3">
              <Label htmlFor="showPlayground">Show playground</Label>
              <Switch id="showPlayground" checked={showPlayground} onCheckedChange={(v) => setShowPlayground(!!v)} />
            </div>
            <div className="flex items-center justify-between gap-3">
              <Label htmlFor="showFile">Show file</Label>
              <Switch id="showFile" checked={showFile} onCheckedChange={(v) => setShowFile(!!v)} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="defaultFile">Default file</Label>
              <Select
                value={defaultFile || undefined}
                onValueChange={(v) => setDefaultFile(v === '__none__' ? '' : v)}
              >
                <SelectTrigger id="defaultFile">
                  <SelectValue placeholder="None" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">None</SelectItem>
                  {editorFiles
                    .map((f) => f.path)
                    .filter((p) => p.endsWith('.baml'))
                    .sort((a, b) => a.localeCompare(b))
                    .map((path) => (
                      <SelectItem key={path} value={path}>
                        {path}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div>
            <Label>How to Use</Label>
            <p className="text-sm text-muted-foreground mt-1">
              Choose how you want to embed your BAML function
            </p>
          </div>

          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <TabsList>
              <TabsTrigger value="link" className="flex items-center gap-2">
                <LinkIcon className="h-4 w-4" />
                Link
              </TabsTrigger>
              <TabsTrigger value="iframe" className="flex items-center gap-2">
                <Code className="h-4 w-4" />
                Iframe
              </TabsTrigger>
              <TabsTrigger value="react" className="flex items-center gap-2">
                <SiReact className="h-4 w-4" />
                React
              </TabsTrigger>
            </TabsList>

            <TabsContent value="link" className="space-y-4 mt-4">
              <Alert>
                <Info className="h-4 w-4" />
                <AlertTitle>Direct link</AlertTitle>
                <AlertDescription>
                  Share the URL directly for users to open in a new tab.
                </AlertDescription>
              </Alert>

              <div className="p-3 bg-background rounded-md border relative">
                <code className="text-sm break-all whitespace-pre-wrap">
                  {generatedUrl || 'YOUR_GENERATED_URL'}
                </code>
                <CopyButton
                  variant="outline"
                  text={generatedUrl || 'YOUR_GENERATED_URL'}
                  className="absolute top-1.5 right-2"
                />
              </div>
            </TabsContent>

            <TabsContent value="iframe" className="space-y-4 mt-4">
              <Alert>
                <Info className="h-4 w-4" />
                <AlertTitle>HTML iframe</AlertTitle>
                <AlertDescription>
                  Use the generated URL in an iframe tag to embed the
                  playground.
                </AlertDescription>
              </Alert>

              <div className="bg-background p-3 rounded-md relative border">
                <code className="text-sm text-pretty break-all whitespace-pre-wrap">
                  {`<iframe
  src="${generatedUrl || 'YOUR_GENERATED_URL'}"
  width="100%"
  height="600px"
  frameborder="0">
</iframe>`}
                </code>
                <CopyButton
                  variant="outline"
                  text={`<iframe
  src="${generatedUrl || 'YOUR_GENERATED_URL'}"
  width="100%"
  height="600px"
  frameborder="0">
</iframe>`}
                  className="absolute top-1.5 right-2"
                />
              </div>
            </TabsContent>

            <TabsContent value="react" className="space-y-4 mt-4">
              <Alert>
                <Info className="h-4 w-4" />
                <AlertTitle>React component</AlertTitle>
                <AlertDescription>
                  Use this component to embed the playground in your React app.
                </AlertDescription>
              </Alert>

              <div className="bg-background p-3 rounded-md relative border">
                <code className="text-sm text-pretty break-all whitespace-pre-wrap">
                  {`import React from 'react';

const BamlPlayground = () => {
  return (
    <iframe
      src="${generatedUrl || 'YOUR_GENERATED_URL'}"
      width="100%"
      height="600px"
      frameBorder="0"
      title="BAML Playground"
    />
  );
};

export default BamlPlayground;`}
                </code>
                <CopyButton
                  variant="outline"
                  text={`import React from 'react';

const BamlPlayground = () => {
  return (
    <iframe
      src="${generatedUrl || 'YOUR_GENERATED_URL'}"
      width="100%"
      height="600px"
      frameBorder="0"
      title="BAML Playground"
    />
  );
};

export default BamlPlayground;`}
                  className="absolute top-1.5 right-2"
                />
              </div>
            </TabsContent>
          </Tabs>
        </div>
      </DialogContent>
    </Dialog>
  );
}
