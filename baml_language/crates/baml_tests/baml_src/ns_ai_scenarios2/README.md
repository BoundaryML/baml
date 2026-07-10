# BEPv2 follow-on scenarios

These examples follow the original 47-scenario corpus and focus on process
boundaries:

- **48** persists an application-owned transcript as JSON across HTTP requests.
- **49** persists only a provider-owned `SessionToken` and explicitly resumes it.
- **50** persists only a background `JobToken` and resumes polling later.

The examples never serialize providers, `Request<T>`, prompt closures, streams,
futures, or live resource objects. Native `workflow` syntax is intentionally not
used because BEPv2 page 10 marks that surface as proposed rather than available
on this branch.
