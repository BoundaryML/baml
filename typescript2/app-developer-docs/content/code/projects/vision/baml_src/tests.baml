// These tests construct requests and parse fixtures. They do not call a model.
function preview_receipt(provider: ai.Client, photo: image) -> string {
    let spec = read_receipt@spec(photo);
    provider
        .render(
            ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal { log: [] },
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            },
        )
        .body
}

test "receipt fields preserve missing values" {
    let receipt = read_receipt@parse(
        `{"merchant":"PAPER CUP CAFE","items":[{"name":"Latte","quantity":1,"price":4.5}],"total":null,"currency":null}`,
    );
    assert.equal(receipt.items[0].price, 4.5);
    assert.equal(receipt.total, null);
    assert.equal(receipt.currency, null)
}

test "each provider receives image content and the receipt schema" {
    let photo = image.from_base64("aW1hZ2U=", "image/png");
    let clients: ai.Client[] = [
        openai.ResponsesClient.new(model = "gpt-4.1-mini", api_key = "test-only"),
        anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-only"),
        google.GeminiClient.new(model = "gemini-2.5-flash", api_key = "test-only"),
        openai.GenericClient.new(model = "test-vision", base_url = "http://localhost:8000/v1"),
    ];
    for (let provider in clients) {
        let body = preview_receipt(provider, photo);
        assert.contains(body, "aW1hZ2U=");
        assert.contains(body, "image/png");
        assert.contains(body, "merchant");
        assert.contains(body, "currency");
    }
}

test "labeled image arrays preserve both images and labels" {
    let spec = ask_images@spec(
        [
            LabeledImage {
                label: "Before",
                photo: image.from_url("https://example.com/before.png", "image/png"),
            },
            LabeledImage {
                label: "After",
                photo: image.from_url("https://example.com/after.png", "image/png"),
            },
        ],
        "What changed?",
    );
    let provider = openai.ResponsesClient.new(model = "gpt-4.1-mini", api_key = "test-only");
    let body = provider
        .render(
            ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal { log: [] },
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            },
        )
        .body;
    assert.contains(body, "Before");
    assert.contains(body, "After");
    assert.contains(body, "https://example.com/before.png");
    assert.contains(body, "https://example.com/after.png");
    assert.contains(body, "What changed?")
}

test "Cloudflare separates the image and preserves system instructions" {
    let provider = CloudflareVisionClient { account_id: "test-account", api_token: "test-secret" };
    let spec = read_receipt@spec(image.from_base64("aW1hZ2U=", "image/png"));
    let request = provider.render(
        ai.ModelTurnInput {
            prompt: spec.prompt_template,
            journal: ai.Journal.new(spec),
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    );
    let body = baml.json.from_string<CloudflareVisionInput>(request.body);
    assert.equal(request.method, "POST");
    assert.contains(
        request.url,
        "/accounts/test-account/ai/run/@cf/meta/llama-3.2-11b-vision-instruct",
    );
    assert.equal(request.headers["Authorization"], "Bearer [redacted]");
    assert.equal(body.image, "data:image/png;base64,aW1hZ2U=");
    assert.equal(body.messages[0].role, "system");
    assert.contains(body.messages[0].content, "merchant");
    assert.equal(body.messages[1].role, "user");
    assert.equal(body.stream, false)
}

test "Cloudflare rejects a second image instead of dropping it" {
    let provider = CloudflareVisionClient { account_id: "test-account", api_token: "test-secret" };
    let photo = image.from_base64("aW1hZ2U=", "image/png");
    let spec = compare_images@spec(photo, photo);
    let outcome = provider.render(
        ai.ModelTurnInput {
            prompt: spec.prompt_template,
            journal: ai.Journal { log: [] },
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    ) catch (error) {
        let invalid: ai.errors.InvalidRequest => invalid.detail,
    };
    assert.equal(outcome, "Provide exactly one image, in a user message.")
}

test "Cloudflare preview does not fetch an image URL" {
    let provider = CloudflareVisionClient { account_id: "test-account", api_token: "test-secret" };
    let outcome = preview_receipt(provider, image.from_url("https://example.invalid/receipt.png", "image/png")) catch (error) {
        let unsupported: ai.errors.PreviewUnsupported => unsupported.detail,
    };
    assert.equal(outcome, "Use a base64 image to preview this request.")
}

test "Cloudflare response becomes a typed receipt" {
    let provider = CloudflareVisionClient { account_id: "test-account", api_token: "test-secret" };
    let response = baml.json.from_string<CloudflareResponse>(
        `{"success":true,"result":{"response":"{\\\"merchant\\\":\\\"PAPER CUP CAFE\\\",\\\"items\\\":[],\\\"total\\\":16.74,\\\"currency\\\":\\\"USD\\\"}"},"errors":[]}`,
    );
    let turn = provider.to_turn(response);
    let receipt = read_receipt@parse(turn.terminal_text() ?? "");
    assert.equal(receipt.total, 16.74);
    assert.equal(receipt.currency, "USD")
}

test "Cloudflare rejects unsuccessful and missing results" {
    let provider = CloudflareVisionClient { account_id: "test-account", api_token: "test-secret" };
    let failed = provider.to_turn(
        CloudflareResponse { success: false, result: null, errors: ["model unavailable"] },
    ) catch (error) {
        let invalid: ai.errors.InvalidRequest => invalid.detail,
    };
    assert.is_type<string>(failed);
    let missing = provider.to_turn(CloudflareResponse { success: true, result: null, errors: [] }) catch (error) {
        let parse: ai.errors.ParseFailed => "missing result",
    };
    assert.equal(missing, "missing result")
}

test "array interpolation and prompt loops preserve label-image pairs" {
    let urls = ["https://example.com/original.png", "https://example.com/updated.png"];
    let labels = ["Original receipt", "Updated receipt"];
    let photos = [
        LabeledImage { label: labels[0], photo: image.from_url(urls[0], "image/png") },
        LabeledImage { label: labels[1], photo: image.from_url(urls[1], "image/png") },
    ];
    let specs = [
        ask_images@spec(photos, "Which items changed?"),
        ask_images_loop@spec(photos, "Which items changed?"),
    ];
    for (let spec in specs) {
        let messages = spec.prompt().messages();
        assert.equal(messages.length(), 2);
        assert.equal(messages[0].role, "system");
        assert.equal(messages[1].role, "user");
        let preceding_text = "";
        let image_count = 0;
        for (let part in messages[1].parts) {
            match (part) {
                let text: string => {
                    preceding_text = preceding_text + text;
                },
                let photo: image => {
                    assert.contains(preceding_text, labels[image_count]);
                    assert.equal(photo.url(), urls[image_count]);
                    image_count += 1;
                    preceding_text = "";
                },
                _ => {
                    assert.is_true(false);
                },
            };
        }
        assert.equal(image_count, 2);
    }
}
