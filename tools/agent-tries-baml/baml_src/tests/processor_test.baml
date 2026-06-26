// Validates a concrete class implements Processor and the pure default-method
// helpers (doc_id / truncate) behave. The full claim loop is exercised against
// the MemoryGateway/FakeProxy harness (integration tier).

class TestProcessor {
    wid: string,
    svc: ServiceClient,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "test",
                table: "trophies",
                claim_value: "queued",
                claim_into: "deduping",
                claim_field: "status",
                claim_index: "by_status_created",
                lease_ms: 1800000,
                heartbeat_ms: 60000,
                poll_backstop_ms: 30000,
                presence_ms: 15000,
                batch: false,
            };
        }

        function service(self) -> ServiceClient {
            return self.svc;
        }

        function worker_id(self) -> string {
            return self.wid;
        }

        function process(self, raw: string) -> int throws ProcessError {
            if (raw == "") {
                throw ProcessError { message: "empty" };
            }
            return 0;
        }
    }
}

function test_proc() -> TestProcessor {
    return TestProcessor { wid: "w1", svc: ServiceClient { base_url: "http://x", token: "" } };
}

testset "processor" {
    test "doc_id extracts _id" {
        assert.equal(test_proc().doc_id(#"{"_id":"abc","status":"queued"}"#), "abc");
    }

    test "doc_id empty when _id absent" {
        assert.equal(test_proc().doc_id(#"{"status":"queued"}"#), "");
    }

    test "config flows through the interface" {
        assert.equal(test_proc().config().table, "trophies");
    }

    test "truncate clips and passes through" {
        assert.equal(truncate("hello", 3), "hel");
        assert.equal(truncate("hi", 5), "hi");
    }
}
