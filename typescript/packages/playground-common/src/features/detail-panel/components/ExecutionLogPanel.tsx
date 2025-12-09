/**
 * ExecutionLogPanel Component
 *
 * Displays a chronological timeline of all execution events as cards.
 * Everything is ordered by timestamp - headers, variables, etc.
 */

import { useAtomValue, useSetAtom } from 'jotai';
import { useEffect, useRef, useMemo } from 'react';
import {
  executionLogAtom,
  scrollToNodeIdAtom,
  detailPanelAtom,
  allNodeStatesAtom,
} from '../../../sdk/atoms/core.atoms';
import type { RichExecutionEvent } from '../../../sdk/interface/events';
import type { NodeExecutionState } from '../../../sdk/types';
import {
  Play,
  Sparkles,
  MessageSquare,
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
  Terminal,
  ChevronDown,
  ChevronRight,
  Hash,
} from 'lucide-react';
import { useState } from 'react';

// ============================================================================
// STATUS INDICATOR
// ============================================================================

function StatusIndicator({ state }: { state: NodeExecutionState }) {
  switch (state) {
    case 'running':
      return <Loader2 className="w-3 h-3 text-blue-500 animate-spin" />;
    case 'error':
      return <XCircle className="w-3 h-3 text-red-500" />;
    case 'pending':
      return <Clock className="w-3 h-3 text-yellow-500" />;
    case 'success':
    default:
      return null;
  }
}


// ============================================================================
// HEADER CARDS
// ============================================================================

function HeaderEnterCard({
  event,
  state,
  isHighlighted,
}: {
  event: RichExecutionEvent;
  state: NodeExecutionState;
  isHighlighted?: boolean;
}) {
  if (event.type !== 'header.enter') return null;

  // Level-based left border colors only
  const levelColors = [
    'border-l-purple-500',
    'border-l-blue-500',
    'border-l-cyan-500',
    'border-l-teal-500',
  ];
  const levelColor = levelColors[(event.level - 1) % levelColors.length];

  return (
    <div
      className={`flex items-center gap-2 px-3 py-1.5 border-l-2 ${levelColor} ${isHighlighted ? 'bg-blue-500/10' : ''}`}
      data-node-id={event.nodeId}
    >
      <StatusIndicator state={state} />
      <span className="text-xs text-muted-foreground">{event.label}</span>
    </div>
  );
}


// ============================================================================
// VARIABLE CARD
// ============================================================================

function VariableCard({ event, isHighlighted }: { event: RichExecutionEvent; isHighlighted?: boolean }) {
  if (event.type !== 'variable.update') return null;

  const displayValue = useMemo(() => {
    if (typeof event.value === 'string') {
      return event.value.length > 200 ? event.value.slice(0, 200) + '...' : event.value;
    }
    const json = JSON.stringify(event.value, null, 2);
    return json.length > 200 ? json.slice(0, 200) + '...' : json;
  }, [event.value]);

  return (
    <div
      className={`px-3 py-1 border-l-2 border-l-blue-400 text-[11px] ${isHighlighted ? 'bg-blue-500/10' : ''}`}
      data-node-id={event.nodeId}
    >
      <span className="text-muted-foreground">{event.name}</span>
      <span className="text-muted-foreground mx-1">=</span>
      <span className="font-mono text-foreground">{displayValue}</span>
    </div>
  );
}

// ============================================================================
// OTHER CARD COMPONENTS
// ============================================================================

function NodeEnterCard({ event, isHighlighted }: { event: RichExecutionEvent; isHighlighted?: boolean }) {
  if (event.type !== 'node.enter') return null;
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div
      className={`border-l-2 border-l-green-500 ${isHighlighted ? 'bg-blue-500/10' : ''}`}
      data-node-id={event.nodeId}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-muted/30"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {isExpanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground" />
        )}
        <Play className="w-3 h-3 text-green-500" />
        <span className="text-xs text-foreground">{event.nodeId}</span>
        <span className="text-[10px] text-muted-foreground ml-auto">
          {new Date(event.timestamp).toLocaleTimeString()}
        </span>
      </div>
      {isExpanded && event.inputs && Object.keys(event.inputs).length > 0 && (
        <div className="px-3 py-2 ml-4">
          <div className="text-[10px] text-muted-foreground mb-1">Inputs:</div>
          <pre className="text-[10px] font-mono overflow-auto max-h-24">
            {JSON.stringify(event.inputs, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function NodeExitCard({ event, isHighlighted }: { event: RichExecutionEvent; isHighlighted?: boolean }) {
  if (event.type !== 'node.exit') return null;
  const [isExpanded, setIsExpanded] = useState(false);
  const hasError = !!event.error;

  return (
    <div
      className={`border-l-2 ${hasError ? 'border-l-red-500' : 'border-l-green-500'} ${isHighlighted ? 'bg-blue-500/10' : ''}`}
      data-node-id={event.nodeId}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5 cursor-pointer hover:bg-muted/30"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {isExpanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground" />
        )}
        {hasError ? (
          <XCircle className="w-3 h-3 text-red-500" />
        ) : (
          <CheckCircle2 className="w-3 h-3 text-green-500" />
        )}
        <span className="text-xs text-foreground">{event.nodeId}</span>
        <span className="text-[10px] text-muted-foreground">
          {event.duration}ms
        </span>
        <span className="text-[10px] text-muted-foreground ml-auto">
          {new Date(event.timestamp).toLocaleTimeString()}
        </span>
      </div>
      {isExpanded && (
        <div className="px-3 py-2 ml-4 space-y-2">
          {event.outputs && Object.keys(event.outputs).length > 0 && (
            <>
              <div className="text-[10px] text-muted-foreground">Output:</div>
              <pre className="text-[10px] font-mono overflow-auto max-h-24">
                {JSON.stringify(event.outputs, null, 2)}
              </pre>
            </>
          )}
          {event.error && (
            <>
              <div className="text-[10px] text-red-500">Error:</div>
              <pre className="text-[10px] font-mono text-red-600 dark:text-red-400 overflow-auto max-h-24">
                {event.error.message}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function LLMRequestCard({ event, isHighlighted }: { event: RichExecutionEvent; isHighlighted?: boolean }) {
  if (event.type !== 'llm.request') return null;
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div
      className={`border border-purple-500/30 rounded-md overflow-hidden ${isHighlighted ? 'ring-2 ring-blue-500' : ''}`}
      data-node-id={event.nodeId}
    >
      <div
        className="flex items-center gap-2 px-3 py-2 bg-purple-500/10 cursor-pointer hover:bg-purple-500/20"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {isExpanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground" />
        )}
        <MessageSquare className="w-3 h-3 text-purple-500" />
        <span className="text-xs font-medium">LLM Request</span>
        <span className="px-1.5 py-0.5 text-[9px] bg-purple-500/20 text-purple-700 dark:text-purple-300 rounded">
          {event.actualClient}
        </span>
      </div>
      {isExpanded && (
        <div className="px-3 py-2 border-t border-purple-500/30 bg-background space-y-2">
          {event.actualModel && (
            <div className="text-[10px]">
              <span className="text-muted-foreground">Model: </span>
              <span className="font-mono">{event.actualModel}</span>
            </div>
          )}
          <pre className="text-[10px] font-mono overflow-auto max-h-32 bg-muted/30 p-2 rounded">
            {JSON.stringify(event.prompt, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function LLMResponseCard({ event, isHighlighted }: { event: RichExecutionEvent; isHighlighted?: boolean }) {
  if (event.type !== 'llm.response') return null;
  const [isExpanded, setIsExpanded] = useState(true);

  return (
    <div
      className={`border border-green-500/30 rounded-md overflow-hidden ${isHighlighted ? 'ring-2 ring-blue-500' : ''}`}
      data-node-id={event.nodeId}
    >
      <div
        className="flex items-center gap-2 px-3 py-2 bg-green-500/10 cursor-pointer hover:bg-green-500/20"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {isExpanded ? (
          <ChevronDown className="w-3 h-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="w-3 h-3 text-muted-foreground" />
        )}
        <Sparkles className="w-3 h-3 text-green-500" />
        <span className="text-xs font-medium">LLM Response</span>
      </div>
      {isExpanded && (
        <div className="px-3 py-2 border-t border-green-500/30 bg-background">
          <pre className="text-[10px] font-mono overflow-auto max-h-48 bg-muted/30 p-2 rounded">
            {JSON.stringify(event.response, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function LogCard({ event }: { event: RichExecutionEvent }) {
  if (event.type !== 'log') return null;

  const levelColors = {
    debug: 'text-gray-500 bg-gray-500/10 border-gray-500/30',
    info: 'text-blue-500 bg-blue-500/10 border-blue-500/30',
    warn: 'text-yellow-600 bg-yellow-500/10 border-yellow-500/30',
    error: 'text-red-500 bg-red-500/10 border-red-500/30',
  };

  return (
    <div className={`flex items-start gap-2 px-3 py-2 border-l-2 rounded-r ${levelColors[event.level]}`}>
      <Terminal className="w-3 h-3 shrink-0 mt-0.5" />
      <div className="flex-1 min-w-0">
        <span className="text-[10px] font-mono uppercase">[{event.level}]</span>
        <span className="text-xs ml-2">{event.message}</span>
      </div>
    </div>
  );
}

// ============================================================================
// COMPUTE INDENTATION LEVELS
// ============================================================================

type EventWithIndent = { event: RichExecutionEvent; indent: number };

function computeIndentLevels(events: RichExecutionEvent[]): EventWithIndent[] {
  const result: EventWithIndent[] = [];

  for (const event of events) {
    if (event.type === 'header.enter') {
      // Use the level from the event (calculated from graph depth)
      result.push({ event, indent: Math.max(0, event.level - 1) });
    } else if (event.type === 'header.exit') {
      result.push({ event, indent: Math.max(0, event.level - 1) });
    } else {
      // Other events don't have indent
      result.push({ event, indent: 0 });
    }
  }

  return result;
}

// ============================================================================
// MAIN COMPONENT
// ============================================================================

export function ExecutionLogPanel() {
  const events = useAtomValue(executionLogAtom);
  const scrollToNodeId = useAtomValue(scrollToNodeIdAtom);
  const setScrollToNodeId = useSetAtom(scrollToNodeIdAtom);
  const detailPanel = useAtomValue(detailPanelAtom);
  const nodeStates = useAtomValue(allNodeStatesAtom);
  const containerRef = useRef<HTMLDivElement>(null);
  const [showOnlyVariables, setShowOnlyVariables] = useState(false);

  // Auto-scroll to bottom when new events arrive
  const prevEventsLengthRef = useRef(events.length);
  useEffect(() => {
    if (events.length > prevEventsLengthRef.current && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
    prevEventsLengthRef.current = events.length;
  }, [events.length]);

  // Scroll to specific node when scrollToNodeId changes
  useEffect(() => {
    if (!scrollToNodeId || !containerRef.current) return;

    const targetElement = containerRef.current.querySelector(
      `[data-node-id="${scrollToNodeId}"]`
    );

    if (targetElement) {
      targetElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
      setTimeout(() => setScrollToNodeId(null), 500);
    }
  }, [scrollToNodeId, setScrollToNodeId]);

  // Filter events if showing only variables
  const filteredEvents = useMemo(() => {
    if (showOnlyVariables) {
      return events.filter(e => e.type === 'variable.update');
    }
    return events;
  }, [events, showOnlyVariables]);

  // Compute indentation levels for all events
  const eventsWithIndent = useMemo(() => computeIndentLevels(filteredEvents), [filteredEvents]);

  if (!detailPanel.isOpen) {
    return null;
  }

  if (events.length === 0) {
    return (
      <div className="h-full flex flex-col bg-card border-t border-border">
        <div className="flex items-center justify-between px-3 py-2 border-b border-border">
          <h3 className="text-xs font-semibold">Execution Log</h3>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center text-center p-4">
          <div className="rounded-full bg-muted/50 p-4 mb-4">
            <Terminal className="w-8 h-8 text-muted-foreground" />
          </div>
          <h3 className="text-sm font-semibold mb-2">No Execution Yet</h3>
          <p className="text-xs text-muted-foreground max-w-sm">
            Run a test to see the execution timeline. Events will appear here in chronological order.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-card border-t border-border">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <h3 className="text-xs font-semibold">Execution Log</h3>
          <span className="px-1.5 py-0.5 text-[10px] bg-muted text-muted-foreground rounded">
            {filteredEvents.length} events
          </span>
        </div>
        <label className="flex items-center gap-1.5 text-[10px] text-muted-foreground cursor-pointer">
          <input
            type="checkbox"
            checked={showOnlyVariables}
            onChange={(e) => setShowOnlyVariables(e.target.checked)}
            className="w-3 h-3 rounded border-border"
          />
          Variables only
        </label>
      </div>

      {/* Event List */}
      <div
        ref={containerRef}
        className="flex-1 overflow-auto p-2 pb-16 space-y-1"
      >
        {eventsWithIndent.map(({ event, indent }, index) => {
          const isHighlighted = event.nodeId === scrollToNodeId;
          const nodeState = nodeStates.get(event.nodeId) || 'not-started';
          // Each indent level = 16px (pl-4)
          const indentStyle = { marginLeft: `${indent * 16}px` };

          switch (event.type) {
            case 'header.enter':
              return (
                <div key={`${event.type}-${event.nodeId}-${index}`} style={indentStyle}>
                  <HeaderEnterCard
                    event={event}
                    state={nodeState}
                    isHighlighted={isHighlighted}
                  />
                </div>
              );
            case 'header.exit':
              // Don't render exit cards
              return null;
            case 'variable.update':
              return (
                <div key={`${event.type}-${event.name}-${index}`} style={indentStyle}>
                  <VariableCard
                    event={event}
                    isHighlighted={isHighlighted}
                  />
                </div>
              );
            case 'node.enter':
              return (
                <div key={`${event.type}-${index}`} style={indentStyle}>
                  <NodeEnterCard event={event} isHighlighted={isHighlighted} />
                </div>
              );
            case 'node.exit':
              return (
                <div key={`${event.type}-${index}`} style={indentStyle}>
                  <NodeExitCard event={event} isHighlighted={isHighlighted} />
                </div>
              );
            case 'llm.request':
              return (
                <div key={`${event.type}-${index}`} style={indentStyle}>
                  <LLMRequestCard event={event} isHighlighted={isHighlighted} />
                </div>
              );
            case 'llm.response':
              return (
                <div key={`${event.type}-${index}`} style={indentStyle}>
                  <LLMResponseCard event={event} isHighlighted={isHighlighted} />
                </div>
              );
            case 'log':
              return (
                <div key={`${event.type}-${index}`} style={indentStyle}>
                  <LogCard event={event} />
                </div>
              );
            default:
              return null;
          }
        })}
      </div>
    </div>
  );
}
