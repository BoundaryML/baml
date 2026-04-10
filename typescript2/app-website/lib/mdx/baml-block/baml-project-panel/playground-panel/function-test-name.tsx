import { ChevronRight, FlaskConical, FunctionSquare } from 'lucide-react';

interface FunctionTestNameProps {
  functionName: string;
  testName: string;
}

export const FunctionTestName: React.FC<FunctionTestNameProps> = ({
  functionName,
  testName,
}) => {
  return (
    <div className="flex items-center space-x-1 w-full text-xs text-muted-foreground/85">
      <div className="flex items-center">
        <FunctionSquare className="w-3 h-3 mr-1" />
        {functionName}
      </div>
      <ChevronRight className="w-3 h-3" />
      <div className="flex items-center">
        <FlaskConical className="w-3 h-3 mr-1" />
        {testName}
      </div>
    </div>
  );
};
