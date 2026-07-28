function resolution_score(value: Resolution) -> float throws never {
    let score = 0.0;
    if (value.category == "billing") {
        score = score + 0.5;
    }
    if (value.reply.length() > 10) {
        score = score + 0.5;
    }
    score
}
