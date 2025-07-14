import { Loader2 } from "lucide-react";
import { cn } from "../lib/utils";

export const Loader: React.FC<{ message?: string; className?: string }> = ({
  message,
  className,
}) => {
  return (
    <div
      className={cn(
        'flex gap-2 justify-center items-center text-muted-foreground',
        className,
      )}
    >
      <Loader2 className="animate-spin" />
      {message}
    </div>
  );
};