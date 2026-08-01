use bex_query::{BqfBuilder, BqfFrame, FrameFlags, FrameKind};

#[test]
fn bqf1_empty_completeness_frame_is_byte_exact_and_frozen() {
    let mut flags = FrameFlags::default();
    flags.insert(FrameFlags::COMPLETE);
    let frame = BqfBuilder::new(FrameKind::Completeness, 17, 23, 0)
        .with_flags(flags)
        .finish(1024)
        .unwrap();
    let expected = decode_hex(include_str!("fixtures/bqf1_empty_completeness.hex"));
    assert_eq!(frame.as_bytes(), expected);

    let decoded = BqfFrame::decode(&expected).unwrap();
    let header = decoded.header().unwrap();
    assert_eq!(header.kind, FrameKind::Completeness);
    assert_eq!(header.request_id, 17);
    assert_eq!(header.data_epoch, 23);
    assert!(header.flags.contains(FrameFlags::COMPLETE));
}

fn decode_hex(value: &str) -> Vec<u8> {
    let value = value.trim();
    assert_eq!(value.len() % 2, 0, "fixture hex must contain whole bytes");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(pair, 16).unwrap()
        })
        .collect()
}
