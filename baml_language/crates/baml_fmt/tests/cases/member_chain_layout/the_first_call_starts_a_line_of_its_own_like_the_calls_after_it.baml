function high_total(inv: Invoice, min_amount: float = 0.0) -> float {
    inv.items.filter((i) -> { i.priority == Priority.High && i.amount >= min_amount }).reduce((a, i) -> { a + i.amount }, 0.0)
}
