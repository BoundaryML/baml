// A non-LLM workload: each request runs a 20-step pipeline of small functions.

class Order {
    id int
    total float
    status string
}

function Validate(order: Order) -> bool {
    order.total > 0.0
}

function Price(order: Order, step: int) -> float {
    order.total * 1.01 + step
}

function Request(id: int, steps: int) -> float {
    let order = Order { id: id, total: 10.0, status: "new" };
    let total = 0.0;
    let i = 0;
    while (i < steps) {
        if (Validate(order)) {
            total += Price(order, i);
        }
        i += 1;
    }
    total
}

function Pipeline(requests: int, steps: int) -> int {
    let done = 0;
    while (done < requests) {
        Request(done, steps);
        done += 1;
    }
    done
}
