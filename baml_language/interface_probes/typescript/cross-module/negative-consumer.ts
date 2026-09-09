import { ResponsesClient, acceptClient, type ClientHost } from "./index.js";

acceptClient(new ResponsesClient());

const rawHost: ClientHost = {
  id: () => "raw",
};
acceptClient(rawHost);
