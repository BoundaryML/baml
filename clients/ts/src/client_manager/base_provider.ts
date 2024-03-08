import { FireBamlEvent } from "../ffi_layer";
import { LLMException } from "./errors";

abstract class BaseProvider {
  protected abstract to_error_code_impl(err: Error | unknown): number | undefined;

  protected to_error_code(err: Error | unknown): number | undefined {
    if (err instanceof LLMException) {
      return err.code;
    }
    const code = this.to_error_code_impl(err);
    if (code !== undefined) {
      return code;
    }

    return undefined;
  }

  protected raise_error(err: Error | unknown): never {
    if (err instanceof Error) {
      const formatted_traceback = err.stack?.split("\n").map((line) => `    ${line}`).join("\n");

      const errorCode = this.to_error_code(err);
      FireBamlEvent.llmError({
        error_code: errorCode ?? 2,
        message: err.message,
        traceback: formatted_traceback,
      });

      if (err instanceof LLMException) {
        throw err;
      }
      if (errorCode !== undefined) {
        throw LLMException.fromError(err, errorCode);
      }
      throw err;
    } else {
      FireBamlEvent.llmError({
        error_code: 2,
        message: `Unknown Error: ${err}`,
      });
      throw LLMException.fromError(err, 2);
    }
  }
}

export { BaseProvider };