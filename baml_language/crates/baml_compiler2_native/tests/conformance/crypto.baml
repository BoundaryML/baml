// `baml.crypto`: SHA-256 and the four AEAD ciphers. Each class keeps Rust
// state the heap owns, and takes and returns `uint8array`s.

function crypto_sha256_empty() -> string {
  baml.crypto.Sha256.new().finish().to_hex()
}
function crypto_sha256_abc() -> string {
  let hasher = baml.crypto.Sha256.new();
  hasher.update(rep("abc", 1).to_utf8());
  hasher.finish().to_hex()
}
function crypto_sha256_in_pieces() -> bool {
  let bytes = u8_counted(1000, 7);
  let whole = baml.crypto.Sha256.new();
  whole.update(bytes);
  let pieces = baml.crypto.Sha256.new();
  for (let i = 0; i < 1000; i += 37) {
    pieces.update(bytes.slice(i, i + 37));
  };
  whole.finish() == pieces.finish()
}
// `finish` leaves the hasher ready for a new message.
function crypto_sha256_finish_resets() -> string {
  let hasher = baml.crypto.Sha256.new();
  hasher.update(u8_counted(10, 1));
  let first = hasher.finish().to_hex();
  let second = hasher.finish().to_hex();
  `${first} ${second}`
}
function crypto_sha256_many() -> string {
  let last = b"";
  for (let i = 0; i < 40; i += 1) {
    let hasher = baml.crypto.Sha256.new();
    hasher.update(last);
    hasher.update(u8_counted(i, 1));
    last = hasher.finish();
  };
  last.to_hex()
}

function crypto_aes256_round_trip() -> string {
  let cipher = baml.crypto.Aes256GcmSiv.new(u8_counted(32, 1));
  let sealed = cipher.encrypt(u8_counted(12, 2), rep("attack at dawn", 2).to_utf8(), b"header");
  let opened = cipher.decrypt(u8_counted(12, 2), sealed, b"header");
  `${sealed.to_hex()} ${opened.to_string()}`
}
function crypto_aes128_round_trip() -> string {
  let cipher = baml.crypto.Aes128GcmSiv.new(u8_counted(16, 3));
  let sealed = cipher.encrypt(u8_counted(12, 5), u8_counted(100, 1), b"");
  let opened = cipher.decrypt(u8_counted(12, 5), sealed, b"");
  `${sealed.length()} ${sealed.slice(0, 20).to_hex()} ${opened == u8_counted(100, 1)}`
}
function crypto_chacha_round_trip() -> string {
  let cipher = baml.crypto.ChaCha20Poly1305.new(u8_counted(32, 7));
  let sealed = cipher.encrypt(u8_counted(12, 1), rep("plain", 5).to_utf8(), u8_counted(3, 1));
  let opened = cipher.decrypt(u8_counted(12, 1), sealed, u8_counted(3, 1));
  `${sealed.to_hex()} ${opened.to_string()}`
}
function crypto_xchacha_round_trip() -> string {
  let cipher = baml.crypto.XChaCha20Poly1305.new(u8_counted(32, 9));
  let sealed = cipher.encrypt(u8_counted(24, 1), rep("plain", 5).to_utf8(), b"aad");
  let opened = cipher.decrypt(u8_counted(24, 1), sealed, b"aad");
  `${sealed.to_hex()} ${opened.to_string()}`
}
function crypto_empty_plaintext() -> string {
  let cipher = baml.crypto.Aes256GcmSiv.new(u8_counted(32, 1));
  let sealed = cipher.encrypt(u8_counted(12, 2), b"", b"");
  `${sealed.to_hex()} ${cipher.decrypt(u8_counted(12, 2), sealed, b"").length()}`
}
// A cipher is used again, across collections.
function crypto_cipher_reused() -> int {
  let cipher = baml.crypto.ChaCha20Poly1305.new(u8_counted(32, 1));
  let total = 0;
  for (let i = 0; i < 30; i += 1) {
    let sealed = cipher.encrypt(u8_counted(12, i), u8_counted(i, 1), b"");
    total += cipher.decrypt(u8_counted(12, i), sealed, b"").length();
  };
  total
}

function crypto_aes256_key_too_short() -> int {
  baml.crypto.Aes256GcmSiv.new(u8_counted(31, 1));
  0
}
function crypto_aes128_key_too_long() -> int {
  baml.crypto.Aes128GcmSiv.new(u8_counted(32, 1));
  0
}
function crypto_chacha_key_empty() -> int {
  baml.crypto.ChaCha20Poly1305.new(b"");
  0
}
function crypto_xchacha_key_wrong() -> int {
  baml.crypto.XChaCha20Poly1305.new(u8_counted(24, 1));
  0
}
function crypto_encrypt_nonce_wrong() -> int {
  baml.crypto.Aes256GcmSiv.new(u8_counted(32, 1)).encrypt(u8_counted(11, 1), b"x", b"").length()
}
function crypto_xchacha_encrypt_nonce_wrong() -> int {
  baml.crypto.XChaCha20Poly1305.new(u8_counted(32, 1)).encrypt(u8_counted(12, 1), b"x", b"").length()
}
function crypto_decrypt_nonce_wrong() -> int {
  baml.crypto.ChaCha20Poly1305.new(u8_counted(32, 1)).decrypt(u8_counted(13, 1), u8_counted(40, 1), b"").length()
}
function crypto_decrypt_too_short() -> int {
  baml.crypto.Aes128GcmSiv.new(u8_counted(16, 1)).decrypt(u8_counted(12, 1), u8_counted(15, 1), b"").length()
}
function crypto_decrypt_tampered() -> int {
  let cipher = baml.crypto.Aes256GcmSiv.new(u8_counted(32, 1));
  let sealed = cipher.encrypt(u8_counted(12, 2), u8_counted(20, 1), b"");
  sealed[3] = sealed[3] + 1;
  cipher.decrypt(u8_counted(12, 2), sealed, b"").length()
}
function crypto_decrypt_wrong_aad() -> int {
  let cipher = baml.crypto.XChaCha20Poly1305.new(u8_counted(32, 1));
  let sealed = cipher.encrypt(u8_counted(24, 2), u8_counted(20, 1), b"one");
  cipher.decrypt(u8_counted(24, 2), sealed, b"two").length()
}
function crypto_decrypt_wrong_key() -> int {
  let sealed = baml.crypto.ChaCha20Poly1305.new(u8_counted(32, 1)).encrypt(u8_counted(12, 2), u8_counted(20, 1), b"");
  baml.crypto.ChaCha20Poly1305.new(u8_counted(32, 2)).decrypt(u8_counted(12, 2), sealed, b"").length()
}
function crypto_decryption_failure_caught() -> string {
  let cipher = baml.crypto.Aes128GcmSiv.new(u8_counted(16, 1));
  let opened = cipher.decrypt(u8_counted(12, 1), u8_counted(16, 1), b"") catch (e) {
    baml.crypto.DecryptionFailure => `${e.algorithm}: ${e.reason}`,
  };
  match (opened) {
    string => opened,
    _ => "opened",
  }
}
function crypto_decryption_failure_short_caught() -> string {
  let cipher = baml.crypto.XChaCha20Poly1305.new(u8_counted(32, 1));
  let opened = cipher.decrypt(u8_counted(24, 1), u8_counted(3, 1), b"") catch (e) {
    baml.crypto.DecryptionFailure => `${e.algorithm}: ${e.reason}`,
  };
  match (opened) {
    string => opened,
    _ => "opened",
  }
}
