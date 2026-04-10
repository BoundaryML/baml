import { useAtom, useAtomValue } from 'jotai';
import { FunctionTestName } from '@/lib/mdx/baml-block/baml-project-panel/playground-panel/function-test-name';
import { Button } from '../../../components/ui/button';
import {
  runtimeAtom,
  safeCurrentIndicesAtom,
  useRunTests,
} from '../lib/learn-atoms';

export default function CodeTrigger() {
  const [runtime] = useAtom(runtimeAtom);
  const { setRunningTests } = useRunTests();
  const { lesson, page } = useAtomValue(safeCurrentIndicesAtom); // Get safe values

  const functions = runtime?.rt?.list_functions() || [];
  const allTestCases = functions.flatMap((fn) =>
    fn.test_cases
      .filter((test) => !test.name.endsWith('_hidden'))
      .map((test) => ({
        displayName: `${fn.name} - ${test.name}`,
        functionName: fn.name,
        testName: test.name,
      })),
  );

  return (
    <div className="flex flex-row gap-2">
      {allTestCases.map((test) => (
        <Button
          key={`${test.functionName}-${test.testName}`}
          onClick={() => {
            console.log(
              `Running test: ${test.functionName} - ${test.testName}`,
            );
            void setRunningTests([
              { functionName: test.functionName, testName: test.testName },
            ]);
          }}
        >
          <FunctionTestName
            functionName={test.functionName}
            testName={test.testName}
          />
        </Button>
      ))}
    </div>
  );
}
