import { cn } from '@/lib/utils';

interface SectionHeaderProps {
  children: React.ReactNode;
  className?: string;
}

export function SectionHeader({ children, className }: SectionHeaderProps) {
  return (
    <div className={cn('w-full h-full p-8 md:p-14', className)}>
      <div className="max-w-xl mx-auto flex flex-col items-center justify-center gap-2">
        {children}
      </div>
    </div>
  );
}
