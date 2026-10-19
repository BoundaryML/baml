import json


class Invalid(Exception):
    pass


def rows_json(n):
    seed = 11
    parts = []
    for i in range(n):
        seed = seed * 16807 % 2147483647
        amount = str(seed % 100000) + (".0" if i % 5 == 0 else "")
        active = "true" if seed % 7 != 0 else "false"
        parts.append('{"account": "acct-%03d", "amount_cents": %s, "active": %s}' % (seed % 97, amount, active))
    return "[" + ", ".join(parts) + "]"


def aggregate(raw):
    totals = {}
    for row in json.loads(raw):
        if not row["account"].isascii():
            raise Invalid("non-ASCII account")
        amount = row["amount_cents"]
        if isinstance(amount, float):
            if amount != int(amount) or amount < 0 or amount > 1000000:
                raise Invalid("invalid amount")
            amount = int(amount)
        if amount < 0 or amount > 1000000:
            raise Invalid("invalid amount")
        if row["active"]:
            totals[row["account"]] = totals.get(row["account"], 0) + amount
    out = [{"account": k, "total_cents": totals[k]} for k in sorted(totals)]
    return json.dumps(out, separators=(",", ":"))


def main():
    raw = rows_json(2048)
    check = 0
    for _ in range(20):
        try:
            out = aggregate(raw)
        except Invalid:
            out = "invalid"
        h = 0
        for ch in out:
            h = (h * 31 + ord(ch)) % 1000000007
        check = (check + h + len(out)) % 1000000007
    return check
