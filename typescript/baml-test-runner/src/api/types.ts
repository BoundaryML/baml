import { z } from "zod";
export enum TestCaseStatus {
  QUEUED = "QUEUED",
  RUNNING = "RUNNING",
  PASSED = "PASSED",
  FAILED = "FAILED",
  CANCELLED = "CANCELLED",
  EXPECTED_FAILURE = "EXPECTED_FAILURE",
}

// These must match DB schema
export const testCaseStatus = z.nativeEnum(TestCaseStatus);
const prefixedUuid = z
  .string()
  .regex(/^[\w]+_[a-f0-9]{8}-([a-f0-9]{4}-){3}[a-f0-9]{12}$/);


const startProcessRequestSchema = z.object({
  project_id: prefixedUuid,
  session_id: prefixedUuid,
  stage: z.string(),
  hostname: z.string(),
  start_time: z.string(),
  tags: z.record(z.string()),
});

const endProcessRequestSchema = z.object({
  project_id: prefixedUuid,
  session_id: prefixedUuid,
  end_time: z.string(),
});

export type StartProcessRequest = z.infer<typeof startProcessRequestSchema>;
export type EndProcessRequest = z.infer<typeof endProcessRequestSchema>;

export const createCycleRequestSchema = z.object({
  project_id: z.string(),
  session_id: z.string(),
});
export type CreateCycleRequest = z.infer<typeof createCycleRequestSchema>;

export const createCycleResponse = z.object({
  test_cycle_id: z.string(),
  dashboard_url: z.string().url(),
});
export type CreateCycleResponse = z.infer<typeof createCycleResponse>;

// For now all strings are valid
const validName = z.string().min(1).max(200);

const testCaseMetadata = z.record(z.any()).and(
  z.object({
    // input: z.string(), // You can define the type of the "input" key here
    name: z.string(),
  })
);

export type CreateTestCaseInputData = z.infer<typeof testCaseMetadata>;


/**
 * e.g.
    {"project_id":"proj_97201c69-296b-4972-9dc2-5fb2e3d206a9","test_cycle_id":"4bca194f-c74b-453c-9248-6ca4a8957084","test_dataset_name":"test_classifytool.py","test_name":"test","test_case_args":[{"name":"test_southern_gold[ClassifyTool-v1]"}]}
 */
export const createTestCaseRequestSchema = z.object({
  project_id: prefixedUuid,
  test_cycle_id: z.string(),
  // the parent block describe() or if it doesn't exist, the filename
  test_dataset_name: validName,
  test_name: z.literal("test"),
  test_case_args: z.array(testCaseMetadata),
});

export type CreateTestCaseRequest = z.infer<typeof createTestCaseRequestSchema>;

/**
 * Example from python tests:
 * {"project_id":"","test_cycle_id":"","test_dataset_name":"test_classifytool.py","test_case_definition_name":"test","test_case_arg_name":"test_southern_gold[ClassifyTool-v1]","status":"RUNNING","error_data":null}
 * 
 */
export const updateTestCaseStatusRequestSchema = z.object({
  project_id: prefixedUuid,
  test_cycle_id: z.string(),
  // the parent block describe() or if it doesn't exist, the filename
  test_dataset_name: validName,
  test_case_definition_name: z.literal("test"),
  test_case_arg_name: validName,
  status: testCaseStatus,
  // result_data: z.record(z.any()).optional().nullable(),
  error_data: z.any().optional().nullable(),
});
export type UpdateTestCaseStatusRequest = z.infer<
  typeof updateTestCaseStatusRequestSchema
>;

export const requestLogTestMetadataSchema = z.object({
  test_cycle_id: prefixedUuid,
  test_dataset_name: validName,
  test_case_name: validName,
  test_case_arg_name: validName,
});

export type RequestLogTestMetadata = z.infer<
  typeof requestLogTestMetadataSchema
>;