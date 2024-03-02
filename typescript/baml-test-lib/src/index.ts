import { BaseReporter, DefaultReporter, Reporter, ReporterContext, ReporterOnStartOptions } from "@jest/reporters"
import type { Circus, Config } from '@jest/types';
import type {
  AggregatedResult,
  SnapshotSummary,
  TestContext,
  Test,
  TestCaseResult,
  TestResult
} from '@jest/test-result';
import { WriteStream } from "tty";

// import { AggregatedResult } from "@jest/reporters"


export class BamlReporter extends BaseReporter implements Reporter {
  protected _globalConfig: Config.GlobalConfig;
  constructor(globalConfig: Config.GlobalConfig) {
    super()
    this.log(JSON.stringify(globalConfig, null, 2))
    this._globalConfig = globalConfig;

  }

  // Called at the beginning of a run
  override onRunStart(aggregatedResults: AggregatedResult, options: ReporterOnStartOptions): void {
    this.log("onRunStart " + JSON.stringify(aggregatedResults, null, 2) + "\n options \n" + JSON.stringify(options, null, 2))
    super.onRunStart(aggregatedResults, options)

  }



  // Called at the beginning of every test file
  override onTestStart(test: Test): void {
    this.log("onTestSTart " + JSON.stringify(test.path, null, 2))
    super.onTestStart(test)

  }

  // Called at the beginning of every .test or .it
  // Doesn't actually work.
  // onTestCaseStart(test: Test, testCaseStartInfo: Circus.TestCaseStartInfo): void {
  //   this.log("onTestCaseStart " + JSON.stringify(testCaseStartInfo, null, 2))
  //   // super.onTestCaseStart(test, testCaseStartInfo)
  // }

  // executed after each test or it() completes
  override onTestCaseResult(test: Test, testCaseResult: TestCaseResult): void {
    this.log("onTestCaseResult " + JSON.stringify(testCaseResult, null, 2))
    super.onTestCaseResult(test, testCaseResult)

  }

  // called with result of every test file
  override onTestResult(
    test: Test,
    testResult: TestResult,
    aggregatedResults: AggregatedResult,
  ): void {
    this.log("onTestresult " + JSON.stringify(testResult, null, 2));

    super.onTestResult(test, testResult, aggregatedResults)
  };

  override async onRunComplete(testContexts: Set<TestContext>, aggregatedResults: AggregatedResult): Promise<void> {
    this.log("onRunComplete " + JSON.stringify(aggregatedResults, null, 2))

    super.onRunComplete(testContexts, aggregatedResults)
  }


}

export default BamlReporter

