import type { ClientInput } from "./client-input.js";
import type { clientInputBrand } from "./internal-brand.js";

// This facade lives in a sibling generated namespace/module.
export class ResponsesClient implements ClientInput {
  declare readonly [clientInputBrand]: true;

  async id(): Promise<string> {
    return "responses";
  }
}
