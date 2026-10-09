// Native backend fixture: bitwise operators and shifts on 63-bit ints.

function band(a: int, b: int) -> int { a & b }

function bor(a: int, b: int) -> int { a | b }

function bxor(a: int, b: int) -> int { a ^ b }

function shl(a: int, n: int) -> int { a << n }

function shr(a: int, n: int) -> int { a >> n }

function mix(a: int, n: int) -> int { ((a << n) | (a >> n)) ^ a }
