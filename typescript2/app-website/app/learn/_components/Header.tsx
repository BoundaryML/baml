import Link from 'next/link';
import {
  Sheet,
  SheetContent,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet';
import TableOfContents from './TableOfContents';

export default function Header() {
  return (
    <div
      className="w-full h-16 px-6 flex flex-row items-center"
      style={{
        background: '#ffffff',
        borderBottom: '1px solid #D9D3C4',
      }}
    >
      <Link
        href="/"
        className="flex-1"
        style={{
          fontWeight: 600,
          fontSize: 18,
          letterSpacing: '0.05em',
          textTransform: 'uppercase',
          color: '#1A1612',
          textDecoration: 'none',
        }}
      >
        Boundary
      </Link>
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
