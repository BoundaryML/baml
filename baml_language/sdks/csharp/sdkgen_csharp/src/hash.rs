pub(crate) fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    use sha2::Digest as _;

    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        })
}
