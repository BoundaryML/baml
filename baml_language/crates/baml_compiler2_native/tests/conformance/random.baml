// `baml.random`: a seed gives the same draws here as in the interpreter,
// and the system's generator is tested by what any draw has.

function random_seed(n: int, step: int) -> uint8array {
  let out = uint8array.zeroes(n);
  for (let i = 0; i < n; i += 1) { out[i] = (i * step + 7) % 256; };
  out
}

function random_xoshiro_bytes() -> string {
  baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 5)).random(20).to_hex()
}
function random_xoshiro_ints() -> string {
  let rng = baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 3));
  `${rng.random_int()} ${rng.random_int()} ${rng.random_int()}`
}
// A draw moves the state: the bytes of two draws are those of one.
function random_xoshiro_in_pieces() -> bool {
  let whole = baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 9)).random(16);
  let pieces = baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 9));
  let first = pieces.random(8);
  first.concat(pieces.random(8)) == whole
}
function random_xoshiro_odd_lengths() -> string {
  let rng = baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 11));
  `${rng.random(0).to_hex()}|${rng.random(1).to_hex()}|${rng.random(3).to_hex()}|${rng.random(5).to_hex()}|${rng.random(13).to_hex()}`
}
// Only the first 32 bytes of a seed are read.
function random_xoshiro_long_seed() -> bool {
  baml.random.Xoshiro256PlusPlus.new(seed = random_seed(40, 5)).random_int() == baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 5)).random_int()
}
function random_xoshiro_zero_seed() -> int {
  baml.random.Xoshiro256PlusPlus.new(seed = baml.Uint8Array.zeroes(32)).random_int()
}
function random_xoshiro_short_seed() -> int {
  baml.random.Xoshiro256PlusPlus.new(seed = random_seed(31, 5)).random_int()
}
function random_xoshiro_negative_count() -> int {
  baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, 5)).random(0 - random_seed(1, 1).length()).length()
}
function random_xoshiro_many() -> int {
  let total = 0;
  for (let i = 0; i < 300; i += 1) {
    total += baml.random.Xoshiro256PlusPlus.new(seed = random_seed(32, i)).random(4).length();
  };
  total
}

function random_chacha_bytes() -> string {
  baml.random.ChaCha20.new(seed = random_seed(32, 5)).random(40).to_hex()
}
function random_chacha_ints() -> string {
  let rng = baml.random.ChaCha20.new(seed = random_seed(32, 3));
  `${rng.random_int()} ${rng.random_int()} ${rng.random_int()}`
}
function random_chacha_bytes_then_int() -> string {
  let rng = baml.random.ChaCha20.new(seed = random_seed(32, 13));
  let bytes = rng.random(3);
  `${bytes.to_hex()} ${rng.random_int()}`
}
function random_chacha_short_seed() -> int {
  baml.random.ChaCha20.new(seed = random_seed(8, 5)).random_int()
}
function random_chacha_negative_count() -> int {
  baml.random.ChaCha20.new(seed = random_seed(32, 5)).random(0 - random_seed(2, 1).length()).length()
}

function random_system_length() -> string {
  let rng = baml.random.SystemRandom.get();
  `${rng.random(0).length()} ${rng.random(1).length()} ${rng.random(33).length()}`
}
// Two draws of 16 bytes are the same once in 2^128 runs.
function random_system_differs() -> bool {
  let rng = baml.random.SystemRandom.get();
  rng.random(16) == rng.random(16)
}
function random_system_negative_count() -> int {
  baml.random.SystemRandom.get().random(0 - random_seed(1, 1).length()).length()
}
function random_system_int_differs() -> bool {
  let rng = baml.random.SystemRandom.get();
  rng.random_int() == rng.random_int()
}
