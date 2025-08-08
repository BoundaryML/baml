'use client';

import { Alert, AlertDescription } from '@baml/ui/alert';
import { Button } from '@baml/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@baml/ui/card';
import { CopyButton } from '@baml/ui/custom/copy-button';
import { Input } from '@baml/ui/input';
import { Label } from '@baml/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@baml/ui/tabs';
import { Textarea } from '@baml/ui/textarea';
import { Code, ExternalLink, Info, Link as LinkIcon } from 'lucide-react';
import dynamic from 'next/dynamic';
import { useEffect, useState } from 'react';
import { SiReact } from 'react-icons/si';
import { createUrl } from '../actions';

const ProjectView = dynamic(
  () => import('../[project_id]/_components/ProjectView'),
  { ssr: false },
);

export default function EmbedPage() {
  const [bamlFunction, setBamlFunction] = useState('');
  const [projectName, setProjectName] = useState('');
  const [generatedUrl, setGeneratedUrl] = useState('');
  const [activeTab, setActiveTab] = useState('link');

  const generateEmbedUrl = async () => {
    if (!bamlFunction.trim() || !projectName.trim()) {
      setGeneratedUrl('');
      return;
    }

    try {
      const project = {
        id: 'embed-project',
        name: projectName,
        description: 'Embedded BAML function',
        files: [
          {
            path: '/main.baml',
            content: bamlFunction,
            error: null,
          },
        ],
      };

      const urlId = await createUrl(project as any);
      const embedUrl = `${window?.location.origin}/embed/${urlId}`;
      setGeneratedUrl(embedUrl);
    } catch (error) {
      console.error('Error generating embed URL:', error);
      setGeneratedUrl('');
    }
  };

  const previewEmbed = () => {
    if (!generatedUrl) return;
    window.open(generatedUrl, '_blank');
  };

  const loadExample = () => {
    setProjectName('Example Person Extractor');
    setBamlFunction(`class ExtractInfo {
  name: string @description("The person's full name")
  age: int @description("The person's age")
  occupation: string @description("The person's job or profession")
}

function extractPersonInfo {
  args {
    text: string @description("The text to extract person information from")
  }
  returns ExtractInfo
  client "openai/gpt-4o"
  prompt #"
    Extract the person's information from the following text.

    Text: {{text}}

    Return the information in the following format:
    - name: The person's full name
    - age: The person's age as a number
    - occupation: The person's job or profession
  "#
}`);
  };

  // Auto-generate URL when inputs change
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    void generateEmbedUrl();
  }, [bamlFunction, projectName]);

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-50 to-slate-100 dark:from-slate-900 dark:to-slate-800">
      <div className="container mx-auto px-4 py-8">
        <div className="max-w-4xl mx-auto">
          {/* Header */}
          <div className="text-center mb-8">
            <h1 className="text-4xl font-bold text-slate-900 dark:text-slate-100 mb-4">
              Create Embed Link
            </h1>
            <p className="text-lg text-slate-600 dark:text-slate-400">
              Generate embeddable links for your BAML functions to share in the
              playground
            </p>
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
            {/* Input Section */}
            <div className="space-y-6">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <Code className="h-5 w-5" />
                    BAML Function
                  </CardTitle>
                  <CardDescription>
                    Enter your BAML function code that you want to embed
                  </CardDescription>
                  <div className="mt-2">
                    <Button
                      onClick={loadExample}
                      variant="outline"
                      size="sm"
                      className="flex items-center gap-2"
                    >
                      <Code className="h-4 w-4" />
                      Load Example
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div>
                    <Label htmlFor="projectName">Project Name</Label>
                    <Input
                      id="projectName"
                      placeholder="My BAML Function"
                      value={projectName}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                        setProjectName(e.target.value)
                      }
                      className="mt-1"
                    />
                  </div>

                  <div>
                    <Label htmlFor="bamlFunction">BAML Function Code</Label>
                    <Textarea
                      id="bamlFunction"
                      placeholder="Enter your BAML function here..."
                      value={bamlFunction}
                      onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
                        setBamlFunction(e.target.value)
                      }
                      className="mt-1 min-h-[300px] font-mono text-sm"
                    />
                  </div>
                </CardContent>
              </Card>
            </div>

            {/* Usage Instructions */}
            <div className="space-y-6">
              <Card>
                <CardHeader>
                  <CardTitle>How to Use</CardTitle>
                  <CardDescription>
                    Choose how you want to embed your BAML function
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex items-center justify-between mb-4">
                    <Tabs
                      value={activeTab}
                      onValueChange={setActiveTab}
                      className="w-full"
                    >
                      <TabsList>
                        <TabsTrigger
                          value="link"
                          className="flex items-center gap-2"
                        >
                          <LinkIcon className="h-4 w-4" />
                          Direct Link
                        </TabsTrigger>
                        <TabsTrigger
                          value="iframe"
                          className="flex items-center gap-2"
                        >
                          <Code className="h-4 w-4" />
                          Iframe
                        </TabsTrigger>
                        <TabsTrigger
                          value="react"
                          className="flex items-center gap-2"
                        >
                          <SiReact className="h-4 w-4" />
                          React
                        </TabsTrigger>
                        <TabsTrigger
                          value="preview"
                          className="flex items-center gap-2"
                        >
                          <ExternalLink className="h-4 w-4" />
                          Preview
                        </TabsTrigger>
                      </TabsList>
                    </Tabs>
                  </div>

                  <Tabs
                    value={activeTab}
                    onValueChange={setActiveTab}
                    className="w-full"
                  >
                    <TabsContent value="link" className="space-y-4 mt-4">
                      <Alert>
                        <Info className="h-4 w-4" />
                        <AlertDescription>
                          <strong>Direct link:</strong> Share the URL directly
                          for users to open in a new tab.
                        </AlertDescription>
                      </Alert>

                      <div className="p-3 bg-background rounded-md border relative">
                        <p className="text-sm text-muted-foreground mb-2">
                          Simply share this URL:
                        </p>
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
                        <AlertDescription>
                          <strong>HTML iframe:</strong> Use the generated URL in
                          an iframe tag to embed the playground.
                        </AlertDescription>
                      </Alert>

                      <div className="bg-background p-3 rounded-md relative">
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
                        <AlertDescription>
                          <strong>React component:</strong> Use this component
                          to embed the playground in your React app.
                        </AlertDescription>
                      </Alert>

                      <div className="bg-background p-3 rounded-md relative">
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

                    <TabsContent value="preview" className="mt-4">
                      {generatedUrl ? (
                        <div
                          className="border rounded-md overflow-hidden"
                          style={{ height: '600px' }}
                        >
                          <ProjectView
                            project={{
                              id: 'embed-project',
                              name: projectName || 'Embedded Project',
                              description: 'Embedded BAML function',
                              files: [
                                {
                                  path: '/main.baml',
                                  content: bamlFunction,
                                },
                              ],
                            }}
                          />
                        </div>
                      ) : (
                        <div className="flex items-center justify-center h-96 bg-background rounded-md border">
                          <p className="text-muted-foreground">
                            Enter a BAML function and project name to see the
                            preview
                          </p>
                        </div>
                      )}
                    </TabsContent>
                  </Tabs>
                </CardContent>
              </Card>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
