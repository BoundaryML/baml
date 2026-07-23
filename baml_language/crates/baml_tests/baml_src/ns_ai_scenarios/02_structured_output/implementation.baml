// Scenario 02 needs no new capability. The return type is the schema and each
// Generate provider decides how to put that schema on its own wire.

class S02Event {
  title: string,
  date: string,
  attendees: string[],
}

class S02OrgNode {
  name: string,
  reports: S02OrgNode[],
}

class S02Approved {
  kind: "approved",
  summary: string,
}

class S02Rejected {
  kind: "rejected",
  reason: string,
}

type S02Decision = S02Approved | S02Rejected
