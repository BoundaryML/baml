"""Deterministic inputs and independent oracles; never timed as workload code."""

import itertools
import json
import math
import random


def encode(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=True)


def matches(actual, expected):
    """JSON structural equality without Python's True == 1 coercion."""
    if type(actual) is bool or type(expected) is bool:
        return type(actual) is type(expected) and actual == expected
    if isinstance(expected, dict):
        return (
            isinstance(actual, dict)
            and actual.keys() == expected.keys()
            and all(matches(actual[k], value) for k, value in expected.items())
        )
    if isinstance(expected, list):
        return (
            isinstance(actual, list)
            and len(actual) == len(expected)
            and all(matches(a, b) for a, b in zip(actual, expected))
        )
    return actual == expected


def fixture(
    case,
    size=1024,
    variant="iterator",
    batches=64,
    retain=4,
    fanout=8,
    delay_ms=10,
    seed=1729,
):
    rng = random.Random(seed)
    if case == "09_json_hello":
        return encode({"message": "Hello, world!"})
    if case == "08_stateful_workflow":
        if fanout not in (1, 4):
            raise ValueError(
                "Stateful workflow uses --fanout 1 or 4 for its four tools"
            )
        requests = []
        for i in range(128):
            request = {
                "key": f"key_{(i - 1 if i % 5 == 1 else i):03d}",
                "mode": "accumulate" if i % 3 else "replace",
                "items": [
                    {
                        "kind": ("credit", "debit", "skip")[j % 3],
                        "amount": rng.randrange(1001),
                        "payload": [rng.randrange(100) for _ in range(16)],
                    }
                    for j in range(size)
                ],
            }
            if i % 17 == 16:
                request["mode"] = "invalid"
            requests.append(encode(request))
        return encode(
            {
                "requests": requests,
                "batch": batches,
                "retain": retain,
                "fanout": fanout,
                "delay_ms": delay_ms,
                "capture": variant == "trace",
            }
        )
    if case in ("07_function_calls", "11_generate_sort"):
        return encode({"count": size, "seed": seed})
    if case == "00_startup":
        return encode({"value": 42})
    if case in ("01_array_traversal", "02_merge_sort", "10_quick_sort", "12_quick_sort_native", "13_sort_kernel_only"):
        values = [rng.randrange(0, 1000000) for _ in range(size)]
        if variant == "sorted":
            values.sort()
        if variant == "reverse":
            values.sort(reverse=True)
        if variant == "duplicates":
            values = [n % 8 for n in values]
        return encode({"values": values, "variant": variant})
    if case in ("03_json_aggregate", "06_http_service"):
        return encode(
            [
                {
                    "account": f"account_{rng.randrange(64):04d}",
                    "amount_cents": rng.randrange(1000001),
                    "active": i % 3 != 0,
                }
                for i in range(size)
            ]
        )
    if case == "04_allocation_retention":
        return encode({"width": size, "batches": batches, "retain": retain})
    if case == "05_async_fanout":
        return encode({"payload_size": size, "fanout": fanout, "delay_ms": delay_ms})
    raise ValueError(case)


def oracle(case, raw, jobs=1, warmup=0):
    try:
        data = json.loads(raw)
    except ValueError:
        if case in ("03_json_aggregate", "06_http_service"):
            return {"error": "invalid"}
        raise
    if case == "09_json_hello":
        return {"message": data["message"]}
    if case == "11_generate_sort":
        # Modular exponentiation reference avoids copying the timed recurrence.
        return sorted(
            data["seed"] * pow(48271, i + 1, 1000000007) % 1000000007
            for i in range(data["count"])
        )
    if case == "08_stateful_workflow":
        return workflow_oracle(data, jobs + warmup)
    if case == "07_function_calls":
        return data["seed"] + data["count"]
    if case == "00_startup":
        return data["value"]
    if case == "01_array_traversal":
        return sum(data["values"])
    if case == "13_sort_kernel_only":
        values = data["values"]
        # This diagnostic returns an extrema checksum after 16 restore/sort
        # rounds. Full ordering is checked by case 12's identical sort kernel.
        return 16 * (min(values) + max(values)) if len(values) > 1 else 0
    if case in ("02_merge_sort", "10_quick_sort", "12_quick_sort_native"):
        return sorted(data["values"])
    if case in ("03_json_aggregate", "06_http_service"):
        # Sort/group, rather than copying the implementations' hash aggregation.
        if not isinstance(data, list) or len(data) > 1000000:
            return {"error": "invalid"}
        for row in data:
            if not isinstance(row, dict):
                return {"error": "invalid"}
            a, n, active = (row.get(k) for k in ("account", "amount_cents", "active"))
            if (
                not isinstance(a, str)
                or not a.isascii()
                or type(active) is not bool
                or type(n) not in (int, float)
                or not math.isfinite(n)
                or n != int(n)
                or not 0 <= n <= 1000000
            ):
                return {"error": "invalid"}
        active = sorted((r for r in data if r["active"]), key=lambda r: r["account"])
        return [
            {"account": key, "total_cents": sum(int(r["amount_cents"]) for r in rows)}
            for key, rows in itertools.groupby(active, key=lambda r: r["account"])
        ]
    if case == "04_allocation_retention":
        w, b = data["width"], data["batches"]
        kept = min(b, data["retain"])
        # Every cell contributes batch + 2*i + 2 after the two alias writes.
        checksum = w * b * (b - 1) // 2 + b * w * (w + 1)
        live = w * kept * (2 * b - kept - 1) // 2 + kept * w * (w + 1)
        return [checksum, live]
    if case == "05_async_fanout":
        n = data["payload_size"]
        return [n * i + n * (n - 1) // 2 for i in range(data["fanout"])]
    raise ValueError(case)


def checks(case):
    if case == "09_json_hello":
        for i, message in enumerate(("", "Hello, world!", 'quote"\\newline\n', "é雪")):
            yield str(i), encode({"message": message})
        return
    if case == "11_generate_sort":
        for size in (0, 1, 17, 1024):
            for seed in (0, 1, 1729, 1000000006):
                yield f"{size}-{seed}", fixture(case, size=size, seed=seed)
        return
    if case == "08_stateful_workflow":
        for size, batch, retain, fanout, delay in (
            (0, 0, 0, 1, 0),
            (0, 8, 0, 1, 0),
            (3, 25, 0, 1, 0),
            (3, 25, 1, 1, 0),
            (7, 33, 4, 1, 0),
            (7, 33, 64, 1, 0),
            (7, 33, 4, 4, 0),
            (7, 8, 4, 1, 1),
            (7, 8, 4, 4, 1),
            (5, 257, 4, 4, 0),
        ):
            yield (
                f"{size}-{batch}-{retain}-{fanout}-{delay}",
                fixture(
                    case,
                    size=size,
                    batches=batch,
                    retain=retain,
                    fanout=fanout,
                    delay_ms=delay,
                    variant="trace",
                ),
            )
        valid = {
            "key": "k",
            "mode": "replace",
            "items": [{"kind": "credit", "amount": 1, "payload": [1, 2]}],
        }
        invalid = [
            "{",
            "null",
            "[]",
            encode(valid | {"key": "é"}),
            encode(valid | {"mode": "bad"}),
        ]
        for field, value in (
            ("kind", "bad"),
            ("amount", -1),
            ("amount", 1000001),
            ("payload", [-1]),
        ):
            invalid.append(
                encode(valid | {"items": [valid["items"][0] | {field: value}]})
            )
        yield (
            "invalid-atomicity",
            encode(
                {
                    "requests": [
                        encode(valid),
                        *invalid,
                        encode(valid | {"mode": "accumulate"}),
                    ],
                    "batch": len(invalid) + 2,
                    "retain": 2,
                    "fanout": 4,
                    "delay_ms": 0,
                    "capture": True,
                }
            ),
        )
        return
    if case == "07_function_calls":
        for n in (0, 1, 128, 8192):
            yield str(n), fixture(case, size=n)
        return
    yield "default", fixture(case, size=16, batches=8, retain=2, delay_ms=2)
    if case in ("01_array_traversal", "02_merge_sort", "10_quick_sort", "12_quick_sort_native", "13_sort_kernel_only"):
        variants = (
            ("iterator", "indexed", "build")
            if case == "01_array_traversal"
            else ("random", "sorted", "reverse", "duplicates")
        )
        for size in (0, 1, 2, 17, 257):
            for variant in variants:
                yield f"{size}-{variant}", fixture(case, size=size, variant=variant)
    if case in ("03_json_aggregate", "06_http_service"):
        examples = [
            [],
            [{"account": "a", "amount_cents": 1.0, "active": True}],
            [{"account": "a", "amount_cents": 1000000, "active": True, "ignored": 5}],
            [{"account": "", "amount_cents": 0, "active": True}],
            [{"account": "__proto__", "amount_cents": 8, "active": True}],
            None,
            {},
            [None],
            [False],
        ]
        for field, values in {
            "account": [None, 4, "é"],
            "amount_cents": [True, "1", -1, 1.5, 1000001, None],
            "active": [None, 1, "true"],
        }.items():
            for value in values:
                row = {"account": "a", "amount_cents": 1, "active": True}
                row[field] = value
                examples.append([row])
            row = {"account": "a", "amount_cents": 1, "active": True}
            del row[field]
            examples.append([row])
        for i, example in enumerate(examples):
            yield f"validation-{i}", encode(example)
        yield "syntax-error", '[{"account":'
        yield "trailing-document", "[] []"
        yield "exponent-integer", '[{"account":"a","amount_cents":1e2,"active":true}]'
    if case == "04_allocation_retention":
        for width, batches, retain in (
            (0, 0, 0),
            (0, 4, 2),
            (5, 9, 0),
            (5, 9, 1),
            (5, 9, 9),
            (5, 9, 20),
        ):
            yield (
                f"{width}-{batches}-{retain}",
                fixture(case, size=width, batches=batches, retain=retain),
            )
    if case == "05_async_fanout":
        for fanout, size in ((0, 4), (1, 0), (1, 7), (32, 8)):
            yield (
                f"{fanout}-{size}",
                fixture(case, fanout=fanout, size=size, delay_ms=2),
            )


def workflow_oracle(config, calls=1):
    """Sequential reference: ordered dictionary eviction and algebraic payload sums."""
    cache = {}
    digest = response_bytes = cursor = 0
    responses = []
    for _ in range(calls):
        responses = []
        for _ in range(config["batch"]):
            raw = config["requests"][cursor % len(config["requests"])]
            response = dict(seq=cursor, ok=False, key="", total=0, check=0, evicted="")
            try:
                req = json.loads(raw)
                assert isinstance(req, dict)
                assert (
                    isinstance(req["key"], str) and req["key"] and req["key"].isascii()
                )
                assert req["mode"] in ("replace", "accumulate") and isinstance(
                    req["items"], list
                )
                contributions = []
                for i, item in enumerate(req["items"]):
                    assert item["kind"] in ("credit", "debit", "skip")
                    assert (
                        type(item["amount"]) is int and 0 <= item["amount"] <= 1000000
                    )
                    assert isinstance(item["payload"], list)
                    assert all(
                        type(v) is int and 0 <= v <= 1000000 for v in item["payload"]
                    )
                    amount = (
                        item["amount"]
                        * {"credit": 1, "debit": -1, "skip": 0}[item["kind"]]
                    )
                    contributions.append(
                        amount
                        + sum(item["payload"])
                        + len(item["payload"]) * (i % 4 + 1)
                    )
                previous = cache.get(req["key"], {"total": 0})["total"]
                total = sum(contributions) + (
                    previous if req["mode"] == "accumulate" else 0
                )
                check = sum((i + 1) * value for i, value in enumerate(contributions))
                evicted = ""
                if config["retain"]:
                    if req["key"] not in cache and len(cache) == config["retain"]:
                        evicted = next(iter(cache))
                        del cache[evicted]
                    cache[req["key"]] = dict(
                        key=req["key"],
                        total=total,
                        check=check,
                        count=len(contributions),
                    )
                response.update(
                    ok=True, key=req["key"], total=total, check=check, evicted=evicted
                )
            except (AssertionError, ValueError, TypeError, KeyError):
                pass
            response_bytes += len(encode(response))
            for value in (
                cursor,
                int(response["ok"]),
                response["total"],
                response["check"],
                len(cache),
            ):
                digest = (digest * 257 + value) % 1000000007
            if config["capture"]:
                responses.append(response)
            cursor += 1
    return dict(
        processed=cursor,
        digest=digest,
        response_bytes=response_bytes,
        cache=[cache[k] for k in sorted(cache)],
        responses=responses,
    )
