import { memo } from 'react';
import { CopyIcon, ThumbDownIcon, ThumbUpIcon } from './icons';
import { Button } from '../components/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../components/tooltip';
import { ChatMessage, MessageActions, getTextFromParts } from './utils';

export interface MessageActionsProps {
  message: ChatMessage;
  isLoading: boolean;
  actions?: MessageActions;
  vote?: { isUpvoted: boolean };
}

function PureMessageActions({
  message,
  isLoading,
  actions,
  vote,
}: MessageActionsProps) {
  if (isLoading) return null;
  if (message.role === 'user') return null;

  const handleCopy = async () => {
    const text = getTextFromParts(message.parts);
    if (!text) {
      console.warn("There's no text to copy!");
      return;
    }

    try {
      await navigator.clipboard.writeText(text);
      console.log('Copied to clipboard!');
    } catch (err) {
      console.error('Failed to copy text: ', err);
    }
    
    actions?.onCopy?.(message);
  };

  const handleUpvote = () => {
    actions?.onFeedback?.(message, 'up');
  };

  const handleDownvote = () => {
    actions?.onFeedback?.(message, 'down');
  };

  return (
    <TooltipProvider delayDuration={0}>
      <div className="flex flex-row gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              className="py-1 px-2 h-fit text-muted-foreground"
              variant="outline"
              onClick={handleCopy}
            >
              <CopyIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Copy</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              data-testid="message-upvote"
              className="py-1 px-2 h-fit text-muted-foreground !pointer-events-auto"
              disabled={vote?.isUpvoted}
              variant="outline"
              onClick={handleUpvote}
            >
              <ThumbUpIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Upvote Response</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              data-testid="message-downvote"
              className="py-1 px-2 h-fit text-muted-foreground !pointer-events-auto"
              variant="outline"
              disabled={vote && !vote.isUpvoted}
              onClick={handleDownvote}
            >
              <ThumbDownIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Downvote Response</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  );
}

export const MessageActionsComponent = memo(PureMessageActions);