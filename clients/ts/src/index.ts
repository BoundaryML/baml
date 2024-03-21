import { FireBamlEvent } from "./ffi_layer";
import * as dotenv from 'dotenv';
import { BamlTestRunner } from "./baml_test_runner";
const setTags = FireBamlEvent.tags;

const loadEnvVars = () => {
  dotenv.config();
}


export { setTags, loadEnvVars };
export { Deserializer, registerEnumDeserializer, registerObjectDeserializer } from "./deserializer/deserializer";
export { clientManager, ConstantDelayRetryPolicy, ExponentialBackoffRetryPolicy } from "./client_manager";
export { trace, traceAsync, FireBamlEvent, BamlTracer } from "./ffi_layer";


export default BamlTestRunner;