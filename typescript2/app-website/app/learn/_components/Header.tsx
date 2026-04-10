import {
  Sheet,
  SheetContent,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet';
import TableOfContents from './TableOfContents';

export default function Header() {
  return (
    <div className="w-full h-16 p-5 bg-purple-500 flex flex-row">
      <h1 className="flex-1">Learn BAML</h1>
      <div className="flex flex-row gap-2">
        <ContentsSheet />
      </div>
    </div>
  );
}

function ContentsSheet() {
  return (
    <Sheet>
      <SheetTrigger>
        <span>Table of Contents</span>
      </SheetTrigger>
      <SheetContent>
        <SheetTitle>Table of Contents</SheetTitle>
        <TableOfContents />
      </SheetContent>
    </Sheet>
  );
}
