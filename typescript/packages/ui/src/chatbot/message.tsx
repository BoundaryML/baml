'use client';
import cx from 'classnames';
import { AnimatePresence, motion } from 'framer-motion';
import { memo, useState } from 'react';
import { PencilEditIcon, SparklesIcon } from './icons';
import { Markdown } from './markdown';
import { MessageActionsComponent } from './message-actions';
import { cn, sanitizeText, ChatMessage, MessageActions, MessagePart } from './utils';
import { Button } from '../components/button';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '../components/tooltip';
import { MessageReasoning } from './message-reasoning';

const PurePreviewMessage = ({
  message,
  vote,
  isLoading,
  actions,
  isReadonly = false,
  requiresScrollPadding = false,
}: {
  message: ChatMessage;
  vote?: { isUpvoted: boolean };
  isLoading: boolean;
  actions?: MessageActions;
  isReadonly?: boolean;
  requiresScrollPadding?: boolean;
}) => {
  const [mode, setMode] = useState<'view' | 'edit'>('view');

  const attachmentsFromMessage = message.parts.filter(
    (part: MessagePart) => part.type === 'file',
  );

  return (
    <AnimatePresence>
      <motion.div
        data-testid={`message-${message.role}`}
        className="w-full mx-auto max-w-3xl px-4 group/message"
        initial={{ y: 5, opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        data-role={message.role}
      >
        <div
          className={cn(
            'flex gap-4 w-full group-data-[role=user]/message:ml-auto group-data-[role=user]/message:max-w-2xl',
            {
              'w-full': mode === 'edit',
              'group-data-[role=user]/message:w-fit': mode !== 'edit',
            },
          )}
        >
          {message.role === 'assistant' && (
            <div className="size-8 flex items-center rounded-full justify-center ring-1 shrink-0 ring-border bg-background">
              <div className="translate-y-px">
                <SparklesIcon size={14} />
              </div>
            </div>
          )}

          <div
            className={cn('flex flex-col gap-4 w-full', {
              'min-h-96': message.role === 'assistant' && requiresScrollPadding,
            })}
          >
            {attachmentsFromMessage.length > 0 && (
              <div
                data-testid={`message-attachments`}
                className="flex flex-row justify-end gap-2"
              >
                {attachmentsFromMessage.map((attachment: MessagePart, index: number) => (
                  <div key={`${attachment.url}-${index}`} className="border rounded p-2 text-sm">
                    📎 {attachment.filename || 'Attachment'}
                  </div>
                ))}
              </div>
            )}

            {message.parts?.map((part: MessagePart, index: number) => {
              const { type } = part;
              const key = `message-${message.id}-part-${index}`;

              if (type === 'reasoning' && part.text && part.text.trim().length > 0) {
                return (
                  <MessageReasoning
                    key={key}
                    isLoading={isLoading}
                    reasoning={part.text}
                  />
                );
              }

              if (type === 'text') {
                if (mode === 'view') {
                  return (
                    <div key={key} className="flex flex-row gap-2 items-start">
                      {message.role === 'user' && !isReadonly && (
                        <TooltipProvider>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                data-testid="message-edit-button"
                                variant="ghost"
                                className="px-2 h-fit rounded-full text-muted-foreground opacity-0 group-hover/message:opacity-100"
                                onClick={() => {
                                  setMode('edit');
                                }}
                              >
                                <PencilEditIcon />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>Edit message</TooltipContent>
                          </Tooltip>
                        </TooltipProvider>
                      )}

                      <div
                        data-testid="message-content"
                        className={cn('flex flex-col gap-4', {
                          'bg-primary text-primary-foreground px-3 py-2 rounded-xl':
                            message.role === 'user',
                        })}
                      >
                        <Markdown>{sanitizeText(part.text || '')}</Markdown>
                      </div>
                    </div>
                  );
                }

                if (mode === 'edit') {
                  return (
                    <div key={key} className="flex flex-row gap-2 items-start">
                      <div className="size-8" />
                      <div className="flex-1 p-3 border rounded-xl">
                        <textarea
                          className="w-full min-h-[100px] resize-none border-0 bg-transparent focus:outline-none"
                          defaultValue={part.text || ''}
                          onBlur={(e) => {
                            // Handle message edit
                            actions?.onEdit?.(message);
                            setMode('view');
                          }}
                          onKeyDown={(e) => {
                            if (e.key === 'Escape') {
                              setMode('view');
                            }
                          }}
                          autoFocus
                        />
                        <div className="flex gap-2 mt-2">
                          <Button size="sm" onClick={() => setMode('view')}>
                            Save
                          </Button>
                          <Button size="sm" variant="outline" onClick={() => setMode('view')}>
                            Cancel
                          </Button>
                        </div>
                      </div>
                    </div>
                  );
                }
              }

              return null;
            })}

            {!isReadonly && (
              <MessageActionsComponent
                key={`action-${message.id}`}
                message={message}
                vote={vote}
                isLoading={isLoading}
                actions={actions}
              />
            )}
          </div>
        </div>
      </motion.div>
    </AnimatePresence>
  );
};

export const PreviewMessage = memo(PurePreviewMessage);

export const ThinkingMessage = () => {
  const role = 'assistant';

  return (
    <motion.div
      data-testid="message-assistant-loading"
      className="w-full mx-auto max-w-3xl px-4 group/message min-h-96"
      initial={{ y: 5, opacity: 0 }}
      animate={{ y: 0, opacity: 1, transition: { delay: 1 } }}
      data-role={role}
    >
      <div
        className={cx(
          'flex gap-4 group-data-[role=user]/message:px-3 w-full group-data-[role=user]/message:w-fit group-data-[role=user]/message:ml-auto group-data-[role=user]/message:max-w-2xl group-data-[role=user]/message:py-2 rounded-xl',
          {
            'group-data-[role=user]/message:bg-muted': true,
          },
        )}
      >
        <div className="size-8 flex items-center rounded-full justify-center ring-1 shrink-0 ring-border">
          <SparklesIcon size={14} />
        </div>

        <div className="flex flex-col gap-2 w-full">
          <div className="flex flex-col gap-4 text-muted-foreground">
            Hmm...
          </div>
        </div>
      </div>
    </motion.div>
  );
};