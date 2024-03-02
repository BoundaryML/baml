import axios, { AxiosError, AxiosResponse } from "axios";
import * as path from "path";
import { string, any } from "zod";
import { CreateCycleRequest, CreateCycleResponse, createCycleResponse, CreateTestCaseRequest, UpdateTestCaseStatusRequest } from "./types";

function logstd(message: string) {
  console.log(message);
}

export class TestingAPI {
  readonly baseUrl: URL;
  readonly processId: string;
  readonly projectId: string;
  readonly apiKey: string;
  private cachedTestCycleInfo: CreateCycleResponse | undefined;
  constructor({
    baseUrl,
    processId,
    apiKey,
    projectId
  }: {
    baseUrl: string;
    processId: string;
    apiKey: string;
    projectId: string;
  }) {
    this.baseUrl = new URL(baseUrl);
    this.processId = processId;
    this.projectId = projectId;
    this.apiKey = apiKey;
  }


  async createTestCycleId(

  ): Promise<CreateCycleResponse> {
    if (this.cachedTestCycleInfo) {
      return this.cachedTestCycleInfo;
    }

    const result = await this.sendRequest("tests/create-cycle", "post", {
      project_id: this.projectId,
      session_id: this.processId,
    });
    const res = createCycleResponse.parse(result.data);
    this.cachedTestCycleInfo = res;
    return res;
  }

  async createTestCases(
    request: Omit<CreateTestCaseRequest, "project_id" | "test_cycle_id">
  ): Promise<void> {
    const fullReq: CreateTestCaseRequest = {
      ...request,
      project_id: this.projectId,
      test_cycle_id: this.cachedTestCycleInfo?.test_cycle_id ?? '',
    };
    await this.sendRequest("tests/create-case", "post", fullReq);
  }

  async updateTestCase(
    request: Omit<UpdateTestCaseStatusRequest, "project_id" | "test_cycle_id">
  ) {
    const fullReq: UpdateTestCaseStatusRequest = {
      ...request,
      project_id: this.projectId,
      test_cycle_id: this.cachedTestCycleInfo?.test_cycle_id ?? '',
    };
    await this.sendRequest("tests/update", "post", fullReq);
  }

  async sendRequest<T>(
    path: string,
    method: "post",
    data: T
  ): Promise<AxiosResponse<any>> {
    try {
      const url = new URL(`${this.baseUrl.toString()}/${path}`);

      const result = await axios({
        method,
        url: new URL(`${this.baseUrl.toString()}/${path}`).toString(),
        data: {
          project_id: process.env.GLOO_PROJECT_ID,
          ...data,
        },
        timeout: 6000,
        headers: {
          Authorization: `Bearer ${this.apiKey}`,
        },
      });

      if (result.status !== 200) {
        logstd(`Error with request to ${path} ${result.statusText}`);
      }
      return result;
    } catch (e) {
      let message = "";
      if (e instanceof AxiosError) {
        message = e.message;
        logstd(
          `Error with request to ${path} ${e.message} ${e.response?.statusText}`
        );
      } else {
        message = JSON.stringify(e).substring(0, 100);
        logstd(
          `Error with request to ${path} ${JSON.stringify(e).substring(0, 100)}`
        );
      }
      throw e;
      // throw new Error(`Error syncing data to the dashboard. ${message}`);
    }
  }

}
