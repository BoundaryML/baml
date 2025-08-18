import { PreviewMessage, ThinkingMessage } from './message';
import { memo } from 'react';
import { motion } from 'framer-motion';
import { ChatMessage, MessageActions } from './utils';

interface MessagesProps {
  status: 'loading' | 'streaming' | 'idle' | 'submitted';
  votes?: Array<{ messageId: string; isUpvoted: boolean }>;
  messages: ChatMessage[];
  actions?: MessageActions;
  isReadonly?: boolean;
  renderGreeting?: () => React.ReactNode;
}

function PureMessages({
  status,
  votes,
  messages,
  actions,
  isReadonly = false,
  renderGreeting,
}: MessagesProps) {

  return (
    <div className="flex flex-col min-w-0 gap-6 flex-1 overflow-y-scroll pt-4 relative">
      {messages.length === 0 && renderGreeting && renderGreeting()}

      {messages.map((message, index) => (
        <PreviewMessage
          key={message.id}
          message={message}
          isLoading={status === 'streaming' && messages.length - 1 === index}
          vote={
            votes
              ? votes.find((vote) => vote.messageId === message.id)
              : undefined
          }
          actions={actions}
          isReadonly={isReadonly}
          requiresScrollPadding={index === messages.length - 1}
        />
      ))}

      {status === 'submitted' &&
        messages.length > 0 &&
        messages[messages.length - 1]?.role === 'user' && <ThinkingMessage />}

      <motion.div
        className="shrink-0 min-w-[24px] min-h-[24px]"
      />
    </div>
  );
}

export const Messages = memo(PureMessages);