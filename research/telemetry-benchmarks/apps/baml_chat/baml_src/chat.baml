// A support chatbot: every turn sends the whole conversation so far to the model.
// The model either answers or asks for an order lookup, whose result is added
// to the conversation before it answers.

client Model = openai.ChatClient.new(
    model = "gpt-4.1-mini",
    api_key = env.MODEL_API_KEY,
    base_url = env.MODEL_BASE_URL,
);

class Msg {
    role: string,
    content: string,
}

class Action {
    kind: "tool" | "reply",
    text: string,
}

class Item {
    sku: string,
    name: string,
    qty: int,
    price: float,
}

class Order {
    id: string,
    status: string,
    carrier: string,
    tracking: string,
    eta: string,
    address: string,
    items: Item[],
}

function Respond(system: string, history: Msg[]) -> Action {
    client: Model
    prompt: `
        ${role("system")}
        ${system}
        ${ctx.output_format()}
        ${for (let m in history)}
            ${role(m.role)}
            ${m.content}
        ${endfor}
    `
}

function LookupOrder(id: string) -> Order {
    Order {
        id: id,
        status: "shipped",
        carrier: "UPS Ground",
        tracking: `1Z999AA1${id}0000784`,
        eta: "2026-11-12",
        address: "1200 Market Street, Apartment 4B, San Francisco, CA 94102, United States",
        items: [
            Item {
                sku: "TRAIL-RUNNER-42",
                name: "Trail running shoe, size 42, slate blue",
                qty: 1,
                price: 129.0,
            },
            Item {
                sku: "WOOL-SOCK-M",
                name: "Merino wool hiking socks, medium, 3-pack",
                qty: 2,
                price: 24.5,
            },
            Item {
                sku: "BOTTLE-750",
                name: "Insulated steel water bottle, 750 ml",
                qty: 1,
                price: 32.0,
            },
        ],
    }
}

// Plays the user's messages in order and returns the number of model calls.
function Converse(system: string, users: string[]) -> int {
    let history: Msg[] = [];
    let calls = 0;
    for (let u in users) {
        history.push(Msg { role: "user", content: u });
        let action = Respond(system, history);
        calls += 1;
        if (action.kind == "tool") {
            history.push(Msg { role: "assistant", content: `Looking up order ${action.text}.` });
            let order = LookupOrder(action.text, $trace = trace.span(inputs = true, output = true));
            history.push(Msg { role: "user", content: `Tool lookup_order returned: ${order}` });
            action = Respond(system, history);
            calls += 1;
        }
        history.push(Msg { role: "assistant", content: action.text });
    }
    calls
}

// The same bot when the customer attaches a photo before the first message.
// The photo stays in the conversation, so every call resends it.
function RespondWithPhoto(system: string, photo: image, history: Msg[]) -> Action {
    client: Model
    prompt: `
        ${role("system")}
        ${system}
        ${ctx.output_format()}
        ${role("user")}
        Here is a photo of the damaged item from my order: ${photo}
        ${for (let m in history)}
            ${role(m.role)}
            ${m.content}
        ${endfor}
    `
}

function ConversePhoto(system: string, photo_b64: string, users: string[]) -> int {
    let photo = image.from_base64(photo_b64, "image/jpeg");
    let history: Msg[] = [];
    let calls = 0;
    for (let u in users) {
        history.push(Msg { role: "user", content: u });
        let action = RespondWithPhoto(system, photo, history);
        calls += 1;
        if (action.kind == "tool") {
            history.push(Msg { role: "assistant", content: `Looking up order ${action.text}.` });
            let order = LookupOrder(action.text, $trace = trace.span(inputs = true, output = true));
            history.push(Msg { role: "user", content: `Tool lookup_order returned: ${order}` });
            action = RespondWithPhoto(system, photo, history);
            calls += 1;
        }
        history.push(Msg { role: "assistant", content: action.text });
    }
    calls
}
