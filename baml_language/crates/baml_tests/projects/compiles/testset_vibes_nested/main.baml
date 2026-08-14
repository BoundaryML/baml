client GPT4o = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    client GPT4o
    prompt `classify ${text}`
}

function GenerateTests(count: int, topic: string) -> string[] {
    client GPT4o
    prompt `generate ${count} tests about ${topic}`
}

testset "test" {
    let topics = ["happy", "sad"];
    for (let sentiments in topics) {
        testset sentiments {
            let req = baml.http.fetch("http://localhost:8000/" + sentiments);
            let data = req.text();
            let tests = GenerateTests$parse(data);
            for (let ex in tests) {
                test ex {
                    let result = ClassifySentiment("hi");
                    assert.equal(result.feeling, "positive");
                }
            }
        }
    }

    testset "vibes" {
        let topics = ["happy", "sad"];
        for (let sentiments in topics) {
            testset sentiments {
                let tests = GenerateTests(5, sentiments);
                for (let ex in tests) {
                    test ex {
                        let result = ClassifySentiment("hi");
                        assert.equal(result.feeling, "positive");
                    }
                }
            }
        }
    }
}

function Foo() -> string | image {
    let x = "foo";
    let y = 2;
    for (let i = 0; i < y; i += 1) {
        x = "hi:  " + x
    }
    x
}

function ClassifySentiment2(text: string) -> string {
    client GPT4o
    prompt `classify ${text}`
}
