use super::*;

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[test]
fn sha1_matches_legacy_cache_hash_vectors() {
    assert_eq!(
        hex(&sha1(b"abc").unwrap()),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(&sha1(b"").unwrap()),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
}

#[test]
fn sha256_matches_fips_180_vectors() {
    assert_eq!(
        hex(&sha256(b"abc").unwrap()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let mut hasher = Sha256::new().unwrap();
    hasher.update(b"hello");
    hasher.update(b" world");
    assert_eq!(
        hex(&hasher.finish()),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    // `finish` resets: the next digest is of the empty message.
    assert_eq!(
        hex(&hasher.finish()),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// RFC 4231 test cases 1, 2 and 6 (6 has a 131-byte key, past the block size).
#[test]
fn hmac_sha256_matches_rfc_4231() {
    assert_eq!(
        hex(&hmac_sha256(&[0x0b; 20], b"Hi There").unwrap()),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?").unwrap()),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )
        .unwrap()),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

/// A throwaway 2048-bit PKCS#8 RSA private key, generated for this test
/// only. Not a credential for anything.
const TEST_RSA_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCuA5xo5IM90XuF
g66lR8fa9HZCBW1BTo8nNZzJOIe4LUCr30IkTZMrwhsUArNHl7mdvi0ZM5ncCdQs
G+IGhF4zzUixw3kQZhzKHl/gu7EERMxmHWyTVRLZoGVe/N194lDlJMPYMga5EPOb
uvNVVo0kPpmjECjZVJcO2FriRVKh1Ly5czUiSZCDy8JKP38VzdS/+EpWXD95CuFb
MHL39aLhLMUtR5i6l4e5VGmMRWosrYvtvhYPKM7L/uafya80B/QvU3RDCbFKBTLu
nuSqXT/q2T4KYnR4CdWgi5dyTTCSGN6VlM1tjn6TgcYvd5FhNtKpW0gPmdjkxRHn
itoZPg/hAgMBAAECggEAA/uF1zS4FKlUy5iuCHKOqrdmPQWv4lIwgheNBSkIKFzP
4YnGL6K7wohInF2hO6XkE/Tn8ODvj0u+SlyV5D1nEvfDcgmdyD8TW5m7eR8dN+eu
SsTh2OwH2kzDj9RpBqFMkHAyaQW12zv9wC84JcotMsN9VxrQkr0yiSz/GidFj2ur
07tVoPM1lIxyBxZRA5xRbku2eV8bKJirWMB8tCxIS5jTp4eA2wz6Qqm3r3T5mMtZ
5Bj4YfxtSzjz8ODoz2VucKrHYnYhZ9VdgAdsHTovQ5WxmcaW27/HAxhU8FyPLzUC
mra3ao1Bq2URzw6gTNrwmWFQBMJCbLgDCuDTyBnFPQKBgQDsSZ6WOi9WDIdgzeAI
x23/9a2qZqgzu6uGz3P2wYaCkDnSDLjlm7x59lBYO2Azh8da9kwbyErumFauuFE1
VqItkwpyVp06N8LbA59rbhzoEY8hN+i6eZhA0p29dFile5eOa76mWZAgIq2Cas/7
PUpB3q/by7U+F/HrEgvOHSLjIwKBgQC8iAU9oLXV2CbUrefucFvSmW8Jpfcz+dw5
jUNIMcpIrIuIM8KnOunZidK4RHmLJpr5tCU5BLaqpjxSN7JLbQLsbXZ0znxhRLl6
QIWCHRRuwlmOTPjLGVJK+pt7xcEwf0ADWRQzaoIOwU00I43PT8RDveI83G7N/Rcp
JbzfLy2DKwKBgDP9btt3Kfsw/oiaQ/UqjFWJZRDdTZ00aeVbuBRPOJ15xn8lNXmv
7qSXQc5oIh60fXTSRKTISVR+SHRhMd0elsiYVfAahrXMlx9BiM5GiC23z1prxtVg
89MvhG2vL+IZc5tusaBAjKnFd4/+mIybS796lA80n0huVFh4vAg5+PcpAoGADP87
jUIVBwb9tk++23s3eU9Gjl24qwagnf8VElcMYPI0NFGNK8Yt9OdBdZ6S2nrw2CIJ
JuMiTKVlJy3bxsNfHjl1nxvVC0eXmcv06EFk9TXEwsCfrjCysaSRL3k0lklPemub
rue6y5Wb4upIjnArUZg3joaLxPubqySE3sX71z0CgYEA0rAmfA6J2coz8Pi3ePDg
SSP8COw3qTRP7LYIOT0LcOMpwmMfhvEA7g8U8dX9012qNo59RCQOQTjCHGZcyfa4
rnWz7YcrTgMZTjR2XZHU4Cny4dGjLf2ndDkS1hNcNWvYkmYssw/R12kO6uPELUVX
U19/pD2CQFHchXuustk5H8Q=
-----END PRIVATE KEY-----";

/// `openssl dgst -sha256 -sign` of `header.claims` with [`TEST_RSA_KEY`].
const TEST_RSA_SIGNATURE: &str = "9b556d5b2b0eb6e7298f26032b719aa80b534d8f381ff1e3fd69e51d81fbf6d8bf8f2d6b0abaf97b03039aaf28993a5c841e1a89e9791f7c8f3beb2b2d70c6c7f2e36bd8051013063155c346104cde5f0b2aa42390a339dd8728cd521364eed5b8c086d83af2d7cf45a1177836edb3069b268b318c98de1d8a129729f894bd3c349867790dd8fc0044806f00cfb207f9ed1269f3d426b2d10c7f9b23828517f08cf8aee252803921e1d88353dcecfecb08ca2e082b113796136cab15737c5530b89a0d14f25ffd5a4f800f2c02c4f3acfacc3dd9d2312de552c4e31523da287d31752a82fa5a092022a33920ac6dc481d9106ae51ba9d588b11c760646add104";

// A fixed key and its OpenSSL signature: PKCS#1 v1.5 signatures are
// deterministic, so the provider must produce exactly these bytes.
#[test]
fn sign_rs256_matches_openssl() {
    assert_eq!(
        hex(&sign_rs256(TEST_RSA_KEY, b"header.claims").unwrap()),
        TEST_RSA_SIGNATURE
    );
}

#[test]
fn sign_rs256_rejects_a_bad_key() {
    assert!(matches!(
        sign_rs256("not a key", b"x"),
        Err(CryptoError::InvalidKey(_))
    ));
}

#[test]
fn fill_random_fills() {
    let mut buf = [0u8; 32];
    fill_random(&mut buf).unwrap();
    assert_ne!(buf, [0u8; 32]);
}

#[test]
fn sign_rs256_accepts_pkcs1_pem() {
    assert_eq!(
        hex(&sign_rs256(
            include_str!("../tests/data/rsa_pkcs1.pem.fixture"),
            b"header.claims"
        )
        .unwrap()),
        TEST_RSA_SIGNATURE
    );
}
