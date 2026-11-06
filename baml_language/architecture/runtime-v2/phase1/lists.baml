class Lists { values: string[][] }
function main() -> int {
  let xs: string[] = ["a", "é"];
  let both = Lists { values: [xs] };
  for (let i = 0; i < 65; i += 1) { xs.push("🦀"); }
  both.values.push(["end"]);
  both.values[0].length() * 100 + xs.join("/").length() + both.values[-1][0].length()
}
