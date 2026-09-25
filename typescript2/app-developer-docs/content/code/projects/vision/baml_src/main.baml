// docs:start ask-image
client Vision = openai.ResponsesClient.new(
    model = "gpt-4.1-mini",
    api_key = env.OPENAI_API_KEY,
);

function ask_image(photo: image, question: string) -> string {
    client: Vision
    prompt: `
        ${role("system")}
        Answer the user's question about the image.
        ${role("user")}
        ${question}
        ${photo}
    `
}

// docs:end ask-image

// docs:start ask-receipt
function ask_receipt() -> string {
    let photo = image.from_file("receipt.png", "image/png");
    ask_image(photo, "Which items on this receipt are drinks?")
}

// docs:end ask-receipt

// docs:start context
function ask_with_context(photo: image, context: string, question: string) -> string {
    client: Vision
    prompt: `
        ${role("system")}
        Answer the user's question using the image and supplied context.
        ${role("user")}
        Context: ${context}
        Image: ${photo}
        Question: ${question}
    `
}

function ask_receipt_with_context() -> string {
    ask_with_context(
        image.from_file("receipt.png", "image/png"),
        "I need to split the drinks from the food on this receipt.",
        "What is the total for drinks, before tax?",
    )
}

// docs:end context

// docs:start compare-images
function compare_images(before: image, after: image) -> string {
    client: Vision
    prompt: `
        ${role("system")}
        Compare the receipts. Identify items that were added or removed.
        ${role("user")}
        Before: ${before}
        After: ${after}
    `
}

function compare_receipts() -> string {
    compare_images(
        image.from_file("receipt.png", "image/png"),
        image.from_file("receipt-updated.png", "image/png"),
    )
}

// docs:end compare-images

// docs:start image-list
class LabeledImage {
    label: string,
    photo: image,
}

function ask_images(photos: LabeledImage[], question: string) -> string {
    client: Vision
    prompt: `
        ${role("system")}
        Answer the user's question using the labeled images.
        ${role("user")}
        ${question}
        ${photos}
    `
}

// docs:end image-list

// docs:start image-list-loop
function ask_images_loop(photos: LabeledImage[], question: string) -> string {
    client: Vision
    prompt: `
        ${role("system")}
        Answer the user's question using the labeled images.
        ${role("user")}
        ${question}
        ${for (let item in photos)}
        ${item.label}:
        ${item.photo}
        ${endfor}
    `
}

// docs:end image-list-loop

// docs:start image-list-call
function compare_labeled_receipts() -> string {
    ask_images(
        [
            LabeledImage {
                label: "Original receipt",
                photo: image.from_file("receipt.png", "image/png"),
            },
            LabeledImage {
                label: "Updated receipt",
                photo: image.from_file("receipt-updated.png", "image/png"),
            },
        ],
        "Which items changed between these receipts?",
    )
}

// docs:end image-list-call

// docs:start receipt-type
class ReceiptItem {
    name: string,
    quantity: int?,
    price: float? @description("Line total, before tax."),
}

class Receipt {
    merchant: string?,
    items: ReceiptItem[],
    total: float?,
    currency: string? @description("Currency code, if shown on the receipt."),
}

// docs:end receipt-type

// docs:start extract
function read_receipt(photo: image) -> Receipt {
    client: Vision
    prompt: `
        ${role("system")}
        Extract the merchant, items, total, and currency from the receipt.
        Use null for missing or unreadable fields. Do not guess.
        ${ctx.output_format()}
        ${role("user")}
        ${photo}
    `
}

// docs:end extract

// docs:start use-result
function receipt_summary() -> string {
    let receipt = read_receipt(image.from_file("receipt.png", "image/png"));
    let names = receipt.items
        .map((item) -> {
            item.name
        })
        .join(", ");
    let total = receipt.total?.to_string() ?? "unreadable";
    `Items: ${names}. Total: ${total}.`
}

// docs:end use-result

// docs:start input-url
function read_url(url: string) -> Receipt {
    read_receipt(image.from_url(url, "image/png"))
}

// docs:end input-url

// docs:start input-file
function read_file(path: string) -> Receipt {
    read_receipt(image.from_file(path, "image/png"))
}

// docs:end input-file

// docs:start input-upload
function read_upload(base64_data: string, mime_type: string) -> Receipt {
    read_receipt(image.from_base64(base64_data, mime_type))
}

// docs:end input-upload

// docs:start openai
function read_with_openai() -> Receipt {
    let model = openai.ResponsesClient.new(model = "gpt-4.1-mini");
    read_receipt(image.from_file("receipt.png", "image/png"), client = model)
}

// docs:end openai

// docs:start anthropic
function read_with_anthropic() -> Receipt {
    let model = anthropic.Client.new(model = "claude-haiku-4-5");
    read_receipt(image.from_file("receipt.png", "image/png"), client = model)
}

// docs:end anthropic

// docs:start google
function read_with_google() -> Receipt {
    let model = google.GeminiClient.new(model = "gemini-2.5-flash");
    read_receipt(image.from_file("receipt.png", "image/png"), client = model)
}

// docs:end google

// docs:start compare-models
class ModelReceipt {
    model: string,
    receipt: Receipt,
}

function compare_models() -> ModelReceipt[] {
    let photo = image.from_file("receipt.png", "image/png");
    let models: ai.Client[] = [
        openai.ResponsesClient.new(model = "gpt-4.1-mini"),
        anthropic.Client.new(model = "claude-haiku-4-5"),
        google.GeminiClient.new(model = "gemini-2.5-flash"),
    ];
    models.map((model) -> {
        ModelReceipt { model: model.id(), receipt: read_receipt(photo, client = model) }
    })
}

// docs:end compare-models

// docs:start custom-provider
client CustomVision = openai.GenericClient.new(
    model = "your-vision-model-id",
    base_url = env.VISION_BASE_URL,
    api_key = env.VISION_API_KEY,
);

function read_with_custom_provider() -> Receipt {
    read_receipt(image.from_file("receipt.png", "image/png"), client = CustomVision)
}

// docs:end custom-provider

// docs:start local-provider
function read_with_local_model(model_id: string) -> Receipt {
    let model = openai.GenericClient.new(model = model_id, base_url = "http://localhost:8000/v1");
    read_receipt(image.from_file("receipt.png", "image/png"), client = model)
}

// docs:end local-provider

// docs:start cloudflare-types
class CloudflareMessage {
    role: string,
    content: string,
}

class CloudflareVisionInput {
    messages: CloudflareMessage[],
    image: string,
    stream: bool,
    max_tokens: int,
}

class CloudflareResult {
    response: string,
}

class CloudflareResponse {
    success: bool,
    result: CloudflareResult?,
    errors: json[],
}

// docs:end cloudflare-types

// docs:start cloudflare-client
class CloudflareVisionClient {
    account_id: string,
    api_token: ai.Credential,

    implements ai.Client {
        function id(self) -> string {
            "cloudflare/llama-3.2-11b-vision-instruct"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            self.build_request(input, preview = true) catch_all (error) {
                _ => throw ai.errors.normalize(error),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            {
                let request = self.build_request(input, preview = false);
                let response = ai.wire.send_as<CloudflareResponse>(request, self.id(), request_timeout_ms = 60000);
                self.to_turn(response)
            } catch_all (error) {
                _ => throw ai.errors.normalize(error),
            }
        }
    }

    function invalid_request(self, detail: string) -> ai.errors.InvalidRequest {
        ai.errors.InvalidRequest { provider: self.id(), status_code: null, detail, raw_body: null }
    }

    function build_request(self, input: ai.ModelTurnInput, preview: bool) -> baml.http.Request {
        let has_history = input.journal.entries().some((event) -> {
            match (event) {
                let started: ai.events.RunStarted => false,
                _ => true,
            }
        });
        if (!input.toolbox.is_empty() || has_history) {
            throw self.invalid_request("This example supports single-turn prompts without tools.");
        }
        let prompt = input.prompt(ai.internal.build_output_format(input.output_type));
        let messages: CloudflareMessage[] = [];
        let image_data: string? = null;
        for (let message in prompt.messages()) {
            if (message.role != "system" && message.role != "user") {
                throw self.invalid_request("This example supports system and user messages.");
            }
            let text: string[] = [];
            for (let part in message.parts) {
                match (part) {
                    let value: string => {
                        text.push(value);
                    },
                    let photo: image => {
                        if (message.role != "user" || image_data != null) {
                            throw self.invalid_request("Provide exactly one image, in a user message.");
                        }
                        let media = if (preview) {
                            ai.wire.resolve_media_preview(photo)
                        } else {
                            ai.wire.resolve_media(photo, fetch_url = true)
                        };
                        let data = media.base64
                            ?? throw ai.errors.PreviewUnsupported {
                            provider: self.id(),
                            detail: "Use a base64 image to preview this request.",
                        };
                        image_data = `data:${media.mime_type};base64,${data}`;
                    },
                    _ => {
                        throw self.invalid_request("This example accepts images, not audio, video, or PDFs.")
                    },
                };
            }
            messages.push(CloudflareMessage { role: message.role, content: text.join("") });
        }
        let photo = image_data ?? throw self.invalid_request("Provide one image.");
        let token = if (preview) {
            "[redacted]"
        } else {
            ai.wire.resolve_credential(self.api_token, null)
                ?? throw self.invalid_request("Set CLOUDFLARE_API_TOKEN.")
        };
        baml.http.Request {
            method: "POST",
            url: `https://api.cloudflare.com/client/v4/accounts/${self.account_id}/ai/run/@cf/meta/llama-3.2-11b-vision-instruct`,
            headers: { "Authorization": `Bearer ${token}`, "Content-Type": "application/json" },
            body: baml.json.to_string(
                CloudflareVisionInput { messages, image: photo, stream: false, max_tokens: 2048 },
            ),
        }
    }

    function to_turn(self, response: CloudflareResponse) -> ai.ModelTurn {
        if (!response.success) {
            throw self.invalid_request(baml.json.to_string(response.errors));
        }
        let result = response.result
            ?? throw ai.errors.ParseFailed {
            provider: self.id(), raw_output: baml.json.to_string(response),
        };
        ai.ModelTurn {
            content: [ai.content.Text { text: result.response }],
            stop_reason: ai.content.StopReason.Complete,
            usage: null,
            calls: [],
        }
    }
}

// docs:end cloudflare-client

// docs:start cloudflare-use
function read_with_cloudflare(account_id: string) -> Receipt {
    let model = CloudflareVisionClient { account_id, api_token: env.CLOUDFLARE_API_TOKEN };
    let photo = image.from_file("receipt.png", "image/png");
    read_receipt(photo, client = model)
}
// docs:end cloudflare-use
