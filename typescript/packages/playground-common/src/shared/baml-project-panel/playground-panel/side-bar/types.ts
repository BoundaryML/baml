export interface FunctionData {
  name: string;
  tests: string[];
}

export interface FunctionItemProps {
  label: string;
  tests: string[];
  searchTerm?: string;
}

export interface TestItemProps {
  label: string;
  isSelected?: boolean;
  searchTerm?: string;
  functionName: string;
}