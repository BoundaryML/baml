import * as React from 'react';
import { Search } from 'lucide-react';
import { Label } from '@baml/ui/label';
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarInput,
} from '@baml/ui/sidebar';

interface SearchFormProps extends React.ComponentProps<'form'> {
  searchTerm: string;
  onSearchChange: (value: string) => void;
}

export function SearchForm({ searchTerm, onSearchChange, ...props }: SearchFormProps) {
  return (
    <form {...props}>
      <SidebarGroup className="py-0">
        <SidebarGroupContent className="relative">
          <Label htmlFor="search" className="sr-only">
            Search
          </Label>
          <SidebarInput
            id="search"
            placeholder="Filter Tests"
            value={searchTerm}
            onChange={(e) => onSearchChange(e.target.value)}
            className="pl-8"
          />
          <Search className="pointer-events-none absolute top-1/2 left-2 size-4 -translate-y-1/2 opacity-50 select-none" />
        </SidebarGroupContent>
      </SidebarGroup>
    </form>
  );
}