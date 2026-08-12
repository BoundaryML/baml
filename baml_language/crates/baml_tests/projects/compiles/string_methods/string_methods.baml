// Test: string method calls inside backtick interpolation produce no diagnostics.

function Upper(s: string) -> string { `${s.to_upper_case()}` }

function StartsWith(s: string) -> string { `${s.starts_with("hello")}` }

function SplitCount(s: string) -> string { `${s.split(",").length()}` }

function Trimmed(s: string) -> string { `${s.trim()}` }
