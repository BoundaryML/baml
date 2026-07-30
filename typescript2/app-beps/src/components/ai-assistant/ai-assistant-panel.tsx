"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { Id } from "../../../convex/_generated/dataModel";
import { BepContent } from "@/components/bep/bep-content";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { VersionAnalysisStatus } from "./version-analysis-status";
import {
  Bot,
  Loader2,
  AlertCircle,
  GitCompare,
  ListChecks,
  Send,
  RotateCcw,
  ChevronDown,
  ChevronUp,
} from "lucide-react";

type QuickAction = "summarize_changes" | "list_addressed_concerns" | "custom";

interface Version {
  _id: Id<"bepVersions">;
  version: number;
  title: string;
  createdAt: number;
  editNote?: string;
}

interface Message {
  role: "user" | "assistant";
  content: string;
}

interface AIAssistantPanelProps {
  bepId: Id<"beps">;
  bepNumber: number;
  versions: Version[];
  currentVersionId: Id<"bepVersions"> | null;
  convexSiteUrl: string;
}

export function AIAssistantPanel({
  bepId,
  bepNumber,
  versions,
  currentVersionId,
  convexSiteUrl,
}: AIAssistantPanelProps) {
  const [fromVersionId, setFromVersionId] = useState<Id<"bepVersions"> | "none">("none");
  const [toVersionId, setToVersionId] = useState<Id<"bepVersions"> | "none">("none");
  const [question, setQuestion] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showVersionSelect, setShowVersionSelect] = useState(false);

  const contentRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Sort versions by version number (descending)
  const sortedVersions = [...versions].sort((a, b) => b.version - a.version);
  const currentVersionNumber =
    sortedVersions.find((version) => version._id === currentVersionId)?.version ??
    null;

  // Auto-scroll to bottom as content streams
  useEffect(() => {
    if (contentRef.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight;
    }
  }, [messages, isStreaming]);

  const formatVersionLabel = (version: Version) => {
    const date = new Date(version.createdAt).toLocaleDateString();
    const isLatest = version._id === currentVersionId;
    return `v${version.version}${isLatest ? " (current)" : ""} - ${date}`;
  };

  const startStreaming = useCallback(
    async (userQuestion: string, quickAction?: QuickAction) => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }

      abortControllerRef.current = new AbortController();
      setIsStreaming(true);
      setError(null);

      // Add user message to chat
      const userMessage: Message = { role: "user", content: userQuestion };
      setMessages((prev) => [...prev, userMessage]);

      // Start with empty assistant message
      setMessages((prev) => [...prev, { role: "assistant", content: "" }]);

      try {
        // Build conversation history for follow-ups (exclude the current exchange)
        const conversationHistory = messages.map((m) => ({
          role: m.role,
          content: m.content,
        }));

        const response = await fetch(
          `${convexSiteUrl}/api/ai/stream-assistant`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              bepId,
              fromVersionId: fromVersionId !== "none" ? fromVersionId : undefined,
              toVersionId: toVersionId !== "none" ? toVersionId : undefined,
              question: userQuestion,
              quickAction,
              conversationHistory: conversationHistory.length > 0 ? conversationHistory : undefined,
            }),
            signal: abortControllerRef.current.signal,
          }
        );

        if (!response.ok) {
          const errorData = await response.json();
          throw new Error(errorData.error || "Failed to start streaming");
        }

        const reader = response.body?.getReader();
        if (!reader) {
          throw new Error("No response body");
        }

        const decoder = new TextDecoder();

        while (true) {
          const { done, value } = await reader.read();

          if (done) break;

          const chunk = decoder.decode(value, { stream: true });

          // Check for completion marker
          if (chunk.includes("---STREAM_COMPLETE---")) {
            const parts = chunk.split("---STREAM_COMPLETE---");
            const contentPart = parts[0];

            if (contentPart) {
              setMessages((prev) => {
                const updated = [...prev];
                const lastIdx = updated.length - 1;
                if (updated[lastIdx]?.role === "assistant") {
                  updated[lastIdx] = {
                    ...updated[lastIdx],
                    content: updated[lastIdx].content + contentPart,
                  };
                }
                return updated;
              });
            }
            break;
          }

          // Check for error marker
          if (chunk.includes("---STREAM_ERROR---")) {
            const errorMatch = chunk.match(/---STREAM_ERROR---\n(.*)\n/);
            const errorMessage = errorMatch?.[1] || "Unknown streaming error";
            setError(errorMessage);
            // Remove the empty assistant message
            setMessages((prev) => prev.slice(0, -1));
            break;
          }

          // Normal content chunk
          setMessages((prev) => {
            const updated = [...prev];
            const lastIdx = updated.length - 1;
            if (updated[lastIdx]?.role === "assistant") {
              updated[lastIdx] = {
                ...updated[lastIdx],
                content: updated[lastIdx].content + chunk,
              };
            }
            return updated;
          });
        }
      } catch (err) {
        if (err instanceof Error && err.name === "AbortError") {
          // Remove the empty assistant message on abort
          setMessages((prev) => {
            if (prev[prev.length - 1]?.role === "assistant" && prev[prev.length - 1]?.content === "") {
              return prev.slice(0, -1);
            }
            return prev;
          });
          return;
        }

        const errorMessage = err instanceof Error ? err.message : "Streaming failed";
        setError(errorMessage);
        // Remove the empty assistant message
        setMessages((prev) => prev.slice(0, -1));
      } finally {
        setIsStreaming(false);
      }
    },
    [bepId, convexSiteUrl, fromVersionId, toVersionId, messages]
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!question.trim() || isStreaming) return;

    const q = question.trim();
    setQuestion("");
    startStreaming(q, "custom");
  };

  const handleQuickAction = (action: QuickAction) => {
    if (isStreaming) return;

    let defaultQuestion = "";
    if (action === "summarize_changes") {
      defaultQuestion = "Summarize the changes between these versions. What was modified, added, or removed?";
    } else if (action === "list_addressed_concerns") {
      defaultQuestion = "List the concerns that were raised and explain how they have been addressed.";
    }

    startStreaming(defaultQuestion, action);
  };

  const handleReset = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    setMessages([]);
    setError(null);
    setQuestion("");
    setFromVersionId("none");
    setToVersionId("none");
  };

  const hasVersionComparison = fromVersionId !== "none" && toVersionId !== "none" && fromVersionId !== toVersionId;
  const hasConversation = messages.length > 0;

  return (
    <Card>
      <CardHeader className="pb-4">
        <CardTitle className="flex items-center gap-2 text-lg">
          <Bot className="h-5 w-5" />
          AI Assistant
          <span className="text-xs font-normal bg-purple-100 text-purple-800 px-1.5 py-0.5 rounded">
            Beta
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <Tabs defaultValue="analysis" className="w-full">
          <TabsList className="grid w-full grid-cols-2 mb-4">
            <TabsTrigger value="analysis">Version Analysis</TabsTrigger>
            <TabsTrigger value="chat">Q&A Chat</TabsTrigger>
          </TabsList>

          <TabsContent value="analysis" className="space-y-4">
            {currentVersionId ? (
              <VersionAnalysisStatus versionId={currentVersionId} />
            ) : (
              <p className="text-sm text-muted-foreground">
                Select a version to see its analysis.
              </p>
            )}
          </TabsContent>

          <TabsContent value="chat" className="space-y-4">
            {/* Version Selection (collapsible) */}
            <div className="space-y-2">
              <button
                type="button"
                onClick={() => setShowVersionSelect(!showVersionSelect)}
                className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                {showVersionSelect ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
                Compare versions (optional)
              </button>

              {showVersionSelect && (
                <div className="grid grid-cols-2 gap-3 pt-2">
                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">From version</Label>
                    <Select
                      value={fromVersionId as string}
                      onValueChange={(value) => setFromVersionId(value as Id<"bepVersions"> | "none")}
                    >
                      <SelectTrigger className="h-9">
                        <SelectValue placeholder="None" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="none">
                          <span className="text-muted-foreground">None</span>
                        </SelectItem>
                        {sortedVersions.map((version) => (
                          <SelectItem key={version._id} value={version._id}>
                            {formatVersionLabel(version)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>

                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">To version</Label>
                    <Select
                      value={toVersionId as string}
                      onValueChange={(value) => setToVersionId(value as Id<"bepVersions"> | "none")}
                    >
                      <SelectTrigger className="h-9">
                        <SelectValue placeholder="None (latest)" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="none">
                          <span className="text-muted-foreground">None (latest)</span>
                        </SelectItem>
                        {sortedVersions.map((version) => (
                          <SelectItem key={version._id} value={version._id}>
                            {formatVersionLabel(version)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              )}
            </div>

            {/* Quick Actions */}
            {!hasConversation && (
              <div className="flex gap-2">
                <button
                  onClick={() => handleQuickAction("summarize_changes")}
                  disabled={isStreaming}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-full border hover:bg-accent transition-colors disabled:opacity-50"
                >
                  <GitCompare className="h-3.5 w-3.5" />
                  Summarize {hasVersionComparison ? "changes" : "proposal"}
                </button>
                <button
                  onClick={() => handleQuickAction("list_addressed_concerns")}
                  disabled={isStreaming}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-full border hover:bg-accent transition-colors disabled:opacity-50"
                >
                  <ListChecks className="h-3.5 w-3.5" />
                  List concerns
                </button>
              </div>
            )}

            {/* Error Display */}
            {error && (
              <div className="flex items-start gap-2 p-3 rounded-lg bg-destructive/10 text-destructive text-sm">
                <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
                <span>{error}</span>
              </div>
            )}

            {/* Conversation */}
            {hasConversation && (
              <div
                ref={contentRef}
                className="space-y-4 max-h-[500px] overflow-y-auto pr-2"
              >
                {messages.map((message, idx) => (
                  <div
                    key={idx}
                    className={`${
                      message.role === "user"
                        ? "bg-muted rounded-lg p-3"
                        : "prose prose-sm max-w-none dark:prose-invert"
                    }`}
                  >
                    {message.role === "user" ? (
                      <p className="text-sm">{message.content}</p>
                    ) : (
                      <>
                        <BepContent
                          content={message.content}
                          linkContext={{
                            bepNumber,
                            isHistorical: false,
                            versionNumber: currentVersionNumber,
                          }}
                        />
                        {isStreaming && idx === messages.length - 1 && (
                          <span className="inline-block w-2 h-4 bg-primary animate-pulse ml-1" />
                        )}
                      </>
                    )}
                  </div>
                ))}
              </div>
            )}

            {/* Input Form */}
            <form onSubmit={handleSubmit} className="space-y-3">
              <div className="relative">
                <Textarea
                  ref={inputRef}
                  value={question}
                  onChange={(e) => setQuestion(e.target.value)}
                  placeholder={hasConversation ? "Ask a follow-up question..." : "Ask a question about this BEP..."}
                  className="min-h-[80px] pr-12 resize-none"
                  disabled={isStreaming}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      handleSubmit(e);
                    }
                  }}
                />
                <Button
                  type="submit"
                  size="icon"
                  disabled={!question.trim() || isStreaming}
                  className="absolute bottom-2 right-2 h-8 w-8"
                >
                  {isStreaming ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Send className="h-4 w-4" />
                  )}
                </Button>
              </div>

              {hasConversation && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={handleReset}
                  disabled={isStreaming}
                  className="text-muted-foreground"
                >
                  <RotateCcw className="h-3.5 w-3.5 mr-1.5" />
                  Start new conversation
                </Button>
              )}
            </form>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  );
}
