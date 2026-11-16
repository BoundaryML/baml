import { optional_args_probe_sync } from "./baml_sdk/index.js";

// @ts-expect-error required arg0 cannot be omitted
optional_args_probe_sync();

// @ts-expect-error defaulted args are passed through the $opts object
optional_args_probe_sync(1, 8);

// @ts-expect-error unknown option key
optional_args_probe_sync(1, { opt3: 1 });

// @ts-expect-error wrong required type
optional_args_probe_sync("x");

// @ts-expect-error wrong option value type
optional_args_probe_sync(1, { opt1: "x" });
