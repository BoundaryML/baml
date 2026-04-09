import { handleTourReplRequest } from '../../server/tour-repl-handler';

export default {
  async fetch(request: Request) {
    return handleTourReplRequest(request);
  },
};
