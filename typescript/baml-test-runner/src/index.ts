import { BaseReporter, DefaultReporter, Reporter, ReporterContext, ReporterOnStartOptions } from "@jest/reporters"
import type { Circus, Config } from '@jest/types';

import TestRunner, {
  OnTestFailure,
  OnTestStart,
  OnTestSuccess,
  Test,
  TestEvents,
  TestRunnerContext,
  TestRunnerOptions,
  TestWatcher,
  UnsubscribeFn,
} from "jest-runner";
import axios, { AxiosError, AxiosResponse } from "axios";
import { TestingAPI } from "./api/client";
import { v4 as uuidv4 } from 'uuid'
import { TestCaseStatus } from "./api/types";
import { initTracer } from "baml-client-lib";
class BamlTestRunner extends TestRunner {
  readonly api?: TestingAPI;
  constructor(globalConfig: Config.GlobalConfig, context: TestRunnerContext) {
    super(globalConfig, context)
    // process.env.BAML_TEST_RUNNER = "HI"

    // console.log("setup!!" + process.pid + JSON.stringify(globalConfig, null, 2) + JSON.stringify(context, null, 2))
    // console.log("------------------" + JSON.stringify(process.env, null, 2));
    if (process.env.BOUNDARY_SECRET && process.env.BOUNDARY_PROJECT_ID && process.env.GLOO_BASE_URL) {
      this.api = new TestingAPI({
        baseUrl: process.env.GLOO_BASE_URL,
        apiKey: process.env.BOUNDARY_SECRET,
        processId: uuidv4(),
        projectId: process.env.BOUNDARY_PROJECT_ID,
      })
    } else {
      // don't fail the tests.

      console.log("no boundary secret!")
    }

  }

  async runTests(
    tests: Array<Test>,
    watcher: TestWatcher,
    // // Available if the runner is a CallbackTestRunner
    // onStart: OnTestStart | undefined,
    // onResult: OnTestSuccess | undefined,
    // onFailure: OnTestFailure | undefined,
    options: TestRunnerOptions,
  ): Promise<void> {
    // TODO: SET THE process id as env var here for all logs to be sent to the same place.
    initTracer();
    if (process.stdout.isTTY && process.stderr.isTTY) {
      process.stderr.write = process.stdout.write;
    }

    process.env.BAML_TEST_RUNNER = "HI"
    const res = await this.api?.createTestCycleId();
    console.log("created test cycle" + JSON.stringify(res, null, 2))
    this.on('test-case-result', async (data) => {
      await this.api?.createTestCases({
        test_name: "test",
        test_dataset_name: "test_dataset",
        test_case_args: [{
          name: data[1].title,
        }]
      });
      await this.api?.updateTestCase({
        test_dataset_name: "test_dataset",
        test_case_definition_name: "test",
        test_case_arg_name: data[1].title,
        // TODO: fix for skip
        status: data[1].status === "passed" ? TestCaseStatus.PASSED : TestCaseStatus.FAILED,

      })
    });

    this.on('test-case-start', (data) => {
      // console.log("test-case-start" + JSON.stringify(data, null, 2))
    });

    // NB: the test name can be retrieved by otel by using `expect.getState().currentTestName`

    // NB: At this point, any logs from the rust logger threads may not be captured in stdout due to some jest weirdness. https://github.com/jestjs/jest/issues/4977
    await super.runTests(tests, watcher, options)

    console.log("finished all tests")
  }

  // on<Name extends keyof TestEvents>(eventName: Name, listener: (eventData: TestEvents[Name]) => void | Promise<void>): UnsubscribeFn {
  //   return super.on(eventName, listener)
  // }

}

// some weird shit here due to es module weirdness with ts-jest
export default BamlTestRunner

