import { ResponsesClient, acceptClient, type ClientHost } from "./index.js";

const generatedProvider = new ResponsesClient();
acceptClient(generatedProvider);

const rawHost: ClientHost = {
  id: () => "raw",
};
// @ts-expect-error Same methods do not establish a checked/generated input.
acceptClient(rawHost);

type PublicSurface = typeof import("./index.js");
type Assert<T extends true> = T;
type _BrandIsNotPublic = Assert<"clientInputBrand" extends keyof PublicSurface ? false : true>;
