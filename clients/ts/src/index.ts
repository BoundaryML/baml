import { FireBamlEvent } from "./ffi_layer";
import dotenv from 'dotenv';
const setTags = FireBamlEvent.tags;

const loadEnvVars = () => {
  dotenv.config();
}

export { setTags, loadEnvVars };