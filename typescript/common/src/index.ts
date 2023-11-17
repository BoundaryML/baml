

export interface RunTestRequest {
  cases: TestCaseInfo[]
}

export interface TestCaseInfo {
  name: string
  input: any
}

export interface RunTestStatus {

}