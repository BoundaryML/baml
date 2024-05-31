import { FireBamlEvent } from "./ffi_layer";
import * as dotenv from 'dotenv';
import { BamlTestRunner } from "./baml_test_runner";
import { DeserializerException, DeserializerError } from "./deserializer/diagnostics";
import { LLMException } from "./client_manager/errors";
import { loadEnvConfig } from "@next/env";
import dotenvFlow from 'dotenv-flow';

const setTags = FireBamlEvent.tags;

const loadEnvVars = () => {
  dotenvFlow.config();
  // NextJS setup
  const projectDir = process.cwd();
  loadEnvConfig(projectDir);
}


export { setTags, loadEnvVars };
export { Deserializer, registerEnumDeserializer, registerObjectDeserializer } from "./deserializer/deserializer";
export { clientManager, ConstantDelayRetryPolicy, ExponentialBackoffRetryPolicy } from "./client_manager";
export { trace, traceAsync, FireBamlEvent, BamlTracer } from "./ffi_layer";
export { DeserializerException, DeserializerError, LLMException };


export default BamlTestRunner;