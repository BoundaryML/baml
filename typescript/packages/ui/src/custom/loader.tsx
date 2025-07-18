import { Loader2 } from "lucide-react";
import { cn } from "../lib/utils";

export const Loader: React.FC<{ message?: string; className?: string }> = ({
  message,
  className,
}) => {
  return (
    <div
      className={cn(
        // VSCode webview: prevent flexbox/scrollbar glitches
        'flex justify-center items-center text-muted-foreground min-w-[80px] min-h-[32px] overflow-hidden',
        className,
      )}
    >
      <Loader2 className="animate-spin w-5 h-5 flex-shrink-0 mr-2" />
      {message && <span className="whitespace-nowrap">{message}</span>}
    </div>
  );
};