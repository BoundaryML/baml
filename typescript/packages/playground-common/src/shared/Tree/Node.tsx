import { cn } from '@baml/ui/lib/utils';
import { useAtom, useSetAtom } from 'jotai';
import { ChevronDown, ChevronRight } from 'lucide-react';
import type React from 'react';
import { useEffect } from 'react';
import type { NodeRendererProps } from 'react-arborist';
import { activeFileAtom } from './atoms';

const Node = ({ node, style, dragHandle }: NodeRendererProps<any>) => {
  const setActiveFile = useSetAtom(activeFileAtom);
  const [activeFile] = useAtom(activeFileAtom);

  useEffect(() => {
    if (node.isSelected && (!node.children || node.children.length === 0)) {
      setActiveFile(node.id);
    }
  }, [node.isSelected]);

  return (
    <div
      className={cn(
        'node-container group relative cursor-pointer overflow-x-clip flex flex-col',
        node.state.isSelected || node.id === activeFile
          ? 'bg-zinc-600 text-white'
          : 'text-muted-foreground',
      )}
      style={style as React.CSSProperties}
      ref={dragHandle}
    >
      <div
        className="flex flex-row items-center w-full gap-x-2"
        onClick={() => node.isInternal && node.toggle()}
      >
        <span className="arrow">
          {node.isLeaf ? null : node.isOpen ? (
            <ChevronDown size={16} />
          ) : (
            <ChevronRight size={16} />
          )}
        </span>
        <span className="text-sm">{node.data.name}</span>
      </div>
    </div>
  );
};

export default Node;
