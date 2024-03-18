import type { Config } from '@jest/types';
import TestRunner, { TestRunnerContext, Test, TestWatcher, TestRunnerOptions } from 'jest-runner';
import { BamlTester, TestCaseStatus } from '@boundaryml/baml-core-ffi';
import { BamlTracer } from "../ffi_layer";

interface BamlTestArgs {
  // [test_name, impl_name, func_name]
  expected_tests: [string, string, string][];
}

const to_test_name = (funcName: string, implName: string, testName: string) => {
  return `test_${testName}[${funcName}-${implName}]`
}

const test_name = (meta: { ancestorTitles: Array<string>, title: string }) => {
  if (meta.ancestorTitles.length != 2) {
    throw Error("Expected 2 ancestor titles, got " + meta.ancestorTitles.length);
  };
  console.log(meta.ancestorTitles, meta.title);

  let [testName, funcName] = meta.ancestorTitles;
  let implName = meta.title;

  if (!funcName.startsWith("function:") || !implName.startsWith("impl:") || !testName.startsWith("test_case:")) {
    throw Error("Unexpected test name format: " + JSON.stringify({
      testName,
      funcName,
      implName,
    }, undefined, 2));
  }

  funcName = funcName.slice("function:".length);
  implName = implName.slice("impl:".length);
  testName = testName.slice("test_case:".length);

  return {
    suite: funcName,
    test: to_test_name(funcName, implName, testName),
  };
}

class BamlTestRunner extends TestRunner {
  private bamlTester: BamlTester;

  constructor(globalConfig: Config.GlobalConfig, context: TestRunnerContext) {
    const bamlConfigFileName = process.env.BAML_TEST_CONFIG_FILE ?? process.argv.filter((arg) => arg.startsWith("--baml-test-config-file=")).at(0)?.split("=")[1];
    if (!bamlConfigFileName) {
      throw new Error("--baml-test-config-file=<filename> must be set");
    }
    // Read the json file and pass it to the BamlTester
    const fileContents: BamlTestArgs = require(bamlConfigFileName);

    // console.log("Tests:", fileContents);

    super(globalConfig, context);
    this.bamlTester = new BamlTester(fileContents.expected_tests.map(([test_name, impl_name, func_name]) => [func_name, to_test_name(func_name, impl_name, test_name)]));
  }

  async runTests(test_files: Array<Test>, watcher: TestWatcher, options: TestRunnerOptions) {
    this.on('test-case-start', async ([name, startInfo]) => {
      const testName = test_name(startInfo);
      await this.bamlTester.updateTestCase(testName.suite, testName.test, TestCaseStatus.Running);
    });
    this.on('test-case-result', async ([name, result]) => {
      const testName = test_name(result);
      await this.bamlTester.updateTestCase(testName.suite, testName.test, result.status === 'passed' ? TestCaseStatus.Passed : TestCaseStatus.Failed)
    });

    await this.bamlTester.start();
    try {
      await super.runTests(test_files, watcher, options);
    } finally {
      await this.bamlTester.end();
    }
  }
}

process.on("beforeExit", () => {
  let now = new Date();
  console.log(`${now} beforeExit`);
  BamlTracer.flush();
  now = new Date();
  console.log(`${now} Done flush`);
});

process.on("exit", () => {
  console.log("exit");
});

export default BamlTestRunner;