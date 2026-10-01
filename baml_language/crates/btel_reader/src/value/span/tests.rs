use std::collections::HashMap;

use baml_type::RealizedTy;
use btel_snapshot::{
    BlobError, BlobScratch, Builder, DecodeLimits, Limits, ShapePolicy, Shaper, Snapshot,
    SnapshotObject as O, SnapshotPool, SnapshotValue as V, decode_blob,
};
use serde_json::json;

use super::*;
use crate::value::{
    CmpOp, Kind, RenderLimits, Rendered, Root, Scalar, Segment, equality, navigate, render_value,
    to_scalar,
};

/// Blobs by ID, as a CAS directory would hold them.
#[derive(Default)]
struct Blobs(HashMap<CasId, Result<Arc<DecodedSnapshot>, CasUnavailable>>);
impl BlobSource for Blobs {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable> {
        self.0
            .get(&id)
            .cloned()
            .unwrap_or(Err(CasUnavailable::Missing))
    }
}
impl Blobs {
    /// Store every blob of a capture and return its root blob.
    fn store(&mut self, snapshot: &Snapshot) -> Arc<DecodedSnapshot> {
        let mut scratch = BlobScratch::default();
        let mut root = None;
        for blob in snapshot.blobs() {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes).unwrap();
            let decoded = Arc::new(decode_blob(&bytes, &DecodeLimits::default()).unwrap());
            assert_eq!(decoded.encoded_len, bytes.len() as u64);
            self.0.insert(blob.id(), Ok(Arc::clone(&decoded)));
            root = Some(decoded);
        }
        root.unwrap()
    }
}

fn cut() -> Shaper {
    Shaper::new(ShapePolicy::Split {
        unit_bytes: 64,
        leaf_bytes: 64,
    })
}

fn capture(shaper: &mut Shaper, build: impl FnOnce(&mut Builder) -> V) -> Snapshot {
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let root = build(&mut b);
    b.finish_value(root, shaper)
}

fn text(b: &mut Builder, content: &str) -> V {
    V::String(b.string(&content.into()).unwrap())
}

fn list(b: &mut Builder, items: &[V]) -> V {
    let id = b.reserve_object().unwrap();
    fill(b, id, items);
    V::Object(id)
}

fn fill(b: &mut Builder, id: btel_snapshot::ObjectId, items: &[V]) {
    let element_type = b.push_type(RealizedTy::Unknown);
    let start = b.value_start();
    for item in items {
        b.push_value(*item);
    }
    let items = b.value_range(start);
    b.set_object(
        id,
        O::List {
            element_type,
            items,
            original_len: items.len(),
        },
    );
}

fn map(b: &mut Builder, entries: &[(&str, V)]) -> V {
    let id = b.reserve_object().unwrap();
    let key_type = b.push_type(RealizedTy::String);
    let value_type = b.push_type(RealizedTy::Unknown);
    let start = b.entry_start();
    for (key, value) in entries {
        b.entry(&(*key).into(), *value);
    }
    let entries = b.entry_range(start);
    b.set_object(
        id,
        O::Map {
            key_type,
            value_type,
            entries,
            original_len: entries.len(),
        },
    );
    V::Object(id)
}

fn bytes(b: &mut Builder, content: &[u8]) -> V {
    let object = b.bytes(content);
    let id = b.reserve_object().unwrap();
    b.set_object(id, object);
    V::Object(id)
}

/// `{note, order: {sku, photo, tags: [..]}, count}` with a note, a photo and
/// an order that are each large enough for a blob of their own.
fn invoice(b: &mut Builder) -> V {
    let note = text(b, &"n".repeat(100));
    let sku = text(b, "A-1");
    let photo = bytes(b, &[5; 100]);
    let first = text(b, "fragile");
    let second = text(b, &"t".repeat(100));
    let tags = list(b, &[first, second]);
    let order = map(b, &[("sku", sku), ("photo", photo), ("tags", tags)]);
    map(b, &[("note", note), ("order", order), ("count", V::Int(2))])
}

fn path(segments: &[&str]) -> Vec<Segment> {
    segments
        .iter()
        .map(|segment| match segment.parse::<i64>() {
            Ok(index) => Segment::Index(index),
            Err(_) => Segment::Key((*segment).to_string()),
        })
        .collect()
}

fn at(blobs: &Blobs, root: &Arc<DecodedSnapshot>, segments: &[&str]) -> Nav {
    navigate(blobs, root, Root::Value, None, &path(segments), 16)
}

fn rendered(blobs: &Blobs, root: &Arc<DecodedSnapshot>, limits: &RenderLimits) -> Rendered {
    let Nav::Value(whole) = at(blobs, root, &[]) else {
        panic!("expected a value")
    };
    render_value(blobs, &whole, limits)
}

fn equal(blobs: &Blobs, left: &Nav, right: &Nav) -> Result<Option<bool>, equality::Error> {
    let captured = |nav| equality::Captured { nav, names: None };
    equality::captured(
        blobs,
        captured(left),
        CmpOp::Eq,
        captured(right),
        &equality::Limits::default(),
    )
}

#[test]
fn a_value_reads_the_same_however_it_is_stored() {
    let mut blobs = Blobs::default();
    let split = capture(&mut cut(), invoice);
    assert_eq!(split.blobs().len(), 5, "note, photo, tag, order, invoice");
    let split = blobs.store(&split);
    let whole = capture(&mut Shaper::new(ShapePolicy::Whole), invoice);
    assert_eq!(whole.blobs().len(), 1);
    let whole = blobs.store(&whole);

    let limits = RenderLimits::default();
    let expected = rendered(&blobs, &whole, &limits);
    assert_eq!(expected.incomplete, None);
    assert_eq!(expected.json["order"]["tags"][0], json!("fragile"));
    assert_eq!(rendered(&blobs, &split, &limits), expected);

    for segments in [
        &["note"][..],
        &["order", "sku"],
        &["order", "tags", "1"],
        &["order", "photo"],
        &["order"],
        &["count"],
        &["order", "absent"],
        &["order", "tags", "7"],
    ] {
        let (found, expected) = (at(&blobs, &split, segments), at(&blobs, &whole, segments));
        assert_eq!(
            to_scalar(&blobs, &found, None, &limits),
            to_scalar(&blobs, &expected, None, &limits),
            "{segments:?}"
        );
        assert_eq!(
            equal(&blobs, &found, &expected),
            Ok((found != Nav::Missing).then_some(true)),
            "{segments:?}"
        );
    }
    let note = to_scalar(&blobs, &at(&blobs, &split, &["note"]), None, &limits);
    assert_eq!(
        (note.scalar, note.kind),
        (Scalar::Text("n".repeat(100)), Kind::String)
    );
    assert_ne!(
        equal(
            &blobs,
            &at(&blobs, &split, &["order"]),
            &at(&blobs, &whole, &[])
        ),
        Ok(Some(true))
    );
}

/// `{photo, link}`: an image whose base64 `content` is stored alone under
/// [`cut`], and one that only names its URL.
fn album(b: &mut Builder, content: &str) -> V {
    let data = b.string(&content.into()).unwrap();
    let url = b.string(&"https://example.test/cat.png".into()).unwrap();
    let media = |b: &mut Builder, source| {
        let mime_type = b.string(&"image/png".into());
        let id = b.reserve_object().unwrap();
        b.set_object(
            id,
            O::Media {
                kind: baml_type::MediaKind::Image,
                mime_type,
                source,
            },
        );
        V::Object(id)
    };
    let photo = media(b, btel_snapshot::MediaSource::Base64 { data });
    let link = media(b, btel_snapshot::MediaSource::Url { url, data: None });
    map(b, &[("photo", photo), ("link", link)])
}

/// The content of the first media object in `blob` that holds any.
fn payload(blob: &DecodedSnapshot) -> MediaPayload {
    blob.objects
        .iter()
        .find_map(|object| {
            let DecodedObject::Media(media) = object else {
                return None;
            };
            media.source.data().cloned()
        })
        .expect("a media object with content")
}

#[test]
fn media_is_described_without_its_content_and_compared_by_it() {
    let mut blobs = Blobs::default();
    let content = "iVBORw0K".repeat(20);
    let split = capture(&mut cut(), |b| album(b, &content));
    assert_eq!(split.blobs().len(), 2, "content, album");
    let content_blob = split.blobs().next().unwrap().id();
    let split = blobs.store(&split);
    let whole = capture(&mut Shaper::new(ShapePolicy::Whole), |b| album(b, &content));
    let whole = blobs.store(&whole);
    let other = capture(&mut Shaper::new(ShapePolicy::Whole), |b| {
        album(b, &"R0lGODlh".repeat(20))
    });
    let other = blobs.store(&other);

    // Describing media needs no content, so without it nothing is unavailable.
    let stored = blobs.0.remove(&content_blob).unwrap();
    let expected = Rendered {
        json: json!({
            "photo": {"$media": "image", "mime": "image/png", "base64_len": 160},
            "link": {"$media": "image", "mime": "image/png", "url": "https://example.test/cat.png"},
        }),
        incomplete: None,
        cut: false,
    };
    for root in [&split, &whole] {
        assert_eq!(rendered(&blobs, root, &RenderLimits::default()), expected);
    }
    assert_eq!(at(&blobs, &split, &["photo", "mime"]), Nav::Missing);
    // Comparing media compares content, which only the missing blob holds.
    let photo = |root| at(&blobs, root, &["photo"]);
    assert_eq!(
        equal(&blobs, &photo(&split), &photo(&whole)),
        Err(equality::Error::Evidence(Unavailable::Blob("cas_missing")))
    );
    blobs.0.insert(content_blob, stored);
    let photo = |root| at(&blobs, root, &["photo"]);
    assert_eq!(
        equal(&blobs, &photo(&split), &photo(&whole)),
        Ok(Some(true))
    );
    assert_eq!(
        equal(&blobs, &photo(&split), &photo(&other)),
        Ok(Some(false))
    );
    let link = |root| at(&blobs, root, &["link"]);
    assert_eq!(equal(&blobs, &link(&split), &link(&other)), Ok(Some(true)));
}

#[test]
fn media_content_is_read_from_its_blob_and_checked_against_its_length() {
    let mut blobs = Blobs::default();
    let content = "iVBORw0K".repeat(20);
    let split = capture(&mut cut(), |b| album(b, &content));
    let content_blob = split.blobs().next().unwrap().id();
    let split = blobs.store(&split);
    let external = payload(&split);
    let MediaPayload::External { child, text_len } = external else {
        panic!("expected content in its own blob, got {external:?}")
    };
    assert_eq!(text_len, 160);
    assert_eq!(
        media_content(&blobs, &split, &external).as_deref(),
        Ok(content.as_str())
    );
    let whole = blobs.store(&capture(&mut Shaper::new(ShapePolicy::Whole), |b| {
        album(b, &content)
    }));
    assert_eq!(
        media_content(&blobs, &whole, &payload(&whole)).as_deref(),
        Ok(content.as_str())
    );
    // The parent's recorded length must match what the child holds.
    let misstated = MediaPayload::External {
        child,
        text_len: 159,
    };
    assert_eq!(
        media_content(&blobs, &split, &misstated),
        Err(Unavailable::Blob("cas_corrupt"))
    );
    // A payload from another blob names a slot this one does not have.
    assert_eq!(
        media_content(&blobs, &whole, &external),
        Err(Unavailable::Blob("cas_corrupt"))
    );
    blobs.0.remove(&content_blob);
    assert_eq!(
        media_content(&blobs, &split, &external),
        Err(Unavailable::Blob("cas_missing"))
    );
}

/// Two lists that hold each other, each with text of its own.
fn ring(b: &mut Builder) -> V {
    let first = b.reserve_object().unwrap();
    let second = b.reserve_object().unwrap();
    let text_first = text(b, &"a".repeat(30));
    let text_second = text(b, &"b".repeat(30));
    fill(b, first, &[V::Object(second), text_first]);
    fill(b, second, &[V::Object(first), text_second]);
    list(b, &[V::Object(first), V::Object(second)])
}

#[test]
fn a_cycle_in_another_blob_keeps_its_shape() {
    let mut blobs = Blobs::default();
    let split = capture(&mut cut(), ring);
    assert_eq!(split.blobs().len(), 2);
    let split = blobs.store(&split);
    let whole = blobs.store(&capture(&mut Shaper::new(ShapePolicy::Whole), ring));

    let limits = RenderLimits::default();
    let expected = rendered(&blobs, &whole, &limits);
    assert_eq!(expected.json[0]["$list"][0]["$list"][0], json!({"$ref": 0}));
    assert_eq!(rendered(&blobs, &split, &limits), expected);
    // Into the first list, around the cycle to it again, then its text.
    let around = ["0", "0", "0", "1"];
    let found = to_scalar(&blobs, &at(&blobs, &split, &around), None, &limits);
    assert_eq!(found.scalar, Scalar::Text("a".repeat(30)));
    assert_eq!(
        found,
        to_scalar(&blobs, &at(&blobs, &whole, &around), None, &limits)
    );
    let whole_value = at(&blobs, &split, &[]);
    assert_eq!(
        equal(&blobs, &whole_value, &whole_value),
        Err(equality::Error::Cycle)
    );
}

#[test]
fn equal_content_in_other_blobs_is_one_value() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), |b| {
        let photos = [(); 2].map(|()| bytes(b, &[5; 100]));
        let notes = [(); 2].map(|()| text(b, &"n".repeat(100)));
        list(b, &[photos[0], notes[0], photos[1], notes[1]])
    }));
    let json = rendered(&blobs, &root, &RenderLimits::default()).json;
    assert_eq!(json[0]["$id"], json!(0));
    assert_eq!(json[2], json!({"$ref": 0}));
    // Text has no identity to share.
    assert_eq!(json[1], json!("n".repeat(100)));
    assert_eq!(json[3], json[1]);
}

#[test]
fn a_blob_that_cannot_be_read_costs_only_its_part() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), invoice));
    let Nav::Value(order) = at(&blobs, &root, &["order"]) else {
        panic!("expected the order")
    };
    let note = root.children[0];
    let photo = order.blob().children[0];
    blobs.0.remove(&note);
    blobs
        .0
        .insert(photo, Err(CasUnavailable::Corrupt(BlobError::Truncated)));

    let limits = RenderLimits::default();
    let missing = Unavailable::Blob("cas_missing");
    let corrupt = Unavailable::Blob("cas_corrupt");
    assert_eq!(at(&blobs, &root, &["note"]), Nav::Unavailable(missing));
    assert_eq!(
        at(&blobs, &root, &["order", "photo"]),
        Nav::Unavailable(corrupt)
    );
    assert_eq!(
        to_scalar(&blobs, &at(&blobs, &root, &["order", "sku"]), None, &limits).scalar,
        Scalar::Text("A-1".into())
    );
    let whole = rendered(&blobs, &root, &limits);
    assert_eq!(whole.incomplete, Some(missing));
    assert_eq!(whole.json["note"], json!({"$unavailable": "cas_missing"}));
    assert_eq!(
        whole.json["order"]["photo"],
        json!({"$unavailable": "cas_corrupt"})
    );
    assert_eq!(whole.json["order"]["tags"][0], json!("fragile"));
    assert_eq!(whole.json["count"], json!(2));

    let order = at(&blobs, &root, &["order"]);
    let presented = to_scalar(&blobs, &order, None, &limits);
    assert_eq!(presented.kind, Kind::Json);
    assert_eq!(presented.incomplete, Some(corrupt));
    assert_eq!(
        equal(&blobs, &order, &order),
        Err(equality::Error::Evidence(corrupt))
    );
    let tags = at(&blobs, &root, &["order", "tags"]);
    assert_eq!(equal(&blobs, &tags, &tags), Ok(Some(true)));
}

#[test]
fn one_operation_reads_a_bounded_number_of_blobs() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), |b| {
        let items: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(|letter| text(b, &letter.repeat(100)))
            .collect();
        list(b, &items)
    }));
    let limits = RenderLimits {
        max_blobs: 2,
        ..RenderLimits::default()
    };
    let bounded = rendered(&blobs, &root, &limits);
    assert_eq!(bounded.incomplete, Some(Unavailable::BlobBudget));
    assert_eq!(
        bounded.json,
        json!(["a".repeat(100), "b".repeat(100), {"$unavailable": "value_blob_budget"}])
    );
    let item = |max_blobs| navigate(&blobs, &root, Root::Value, None, &path(&["2"]), max_blobs);
    assert_eq!(item(0), Nav::Unavailable(Unavailable::BlobBudget));
    assert!(matches!(item(1), Nav::Value(_)));
    let whole = at(&blobs, &root, &[]);
    let captured = equality::Captured {
        nav: &whole,
        names: None,
    };
    assert_eq!(
        equality::captured(
            &blobs,
            captured,
            CmpOp::Eq,
            captured,
            &equality::Limits {
                max_blobs: 2,
                ..equality::Limits::default()
            }
        ),
        Err(equality::Error::Evidence(Unavailable::BlobBudget))
    );
}

#[test]
fn one_operation_reads_a_bounded_size_of_blobs() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), |b| {
        let items: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(|letter| text(b, &letter.repeat(100)))
            .collect();
        list(b, &items)
    }));
    let item = blobs.0[&root.children[0]].as_ref().unwrap().encoded_len;
    let render = |max_blob_bytes| {
        let limits = RenderLimits {
            max_blob_bytes,
            ..RenderLimits::default()
        };
        rendered(&blobs, &root, &limits)
    };
    let unread = json!({"$unavailable": "value_blob_budget"});
    assert_eq!(render(0).json, json!([unread, unread, unread]));
    assert_eq!(render(item).json, json!(["a".repeat(100), unread, unread]));
    // A blob's size is known once it is read, so the one that reaches the
    // limit is the last.
    let bounded = render(item + 1);
    assert_eq!(
        bounded.json,
        json!(["a".repeat(100), "b".repeat(100), unread])
    );
    assert_eq!(bounded.incomplete, Some(Unavailable::BlobBudget));
    assert_eq!(render(2 * item + 1).incomplete, None);

    let whole = at(&blobs, &root, &[]);
    let captured = equality::Captured {
        nav: &whole,
        names: None,
    };
    let equal = |max_blob_bytes| {
        let limits = equality::Limits {
            max_blob_bytes,
            ..equality::Limits::default()
        };
        equality::captured(&blobs, captured, CmpOp::Eq, captured, &limits)
    };
    assert_eq!(
        equal(2 * item),
        Err(equality::Error::Evidence(Unavailable::BlobBudget))
    );
    assert_eq!(equal(2 * item + 1), Ok(Some(true)));
}

#[test]
fn a_rendering_is_bounded_however_often_it_reaches_one_blob() {
    let mut blobs = Blobs::default();
    let capture = capture(&mut cut(), |b| {
        let note = text(b, &"n".repeat(100));
        list(b, &[note; 50])
    });
    assert_eq!(capture.blobs().count(), 2, "the text is stored once");
    let root = blobs.store(&capture);
    let whole = rendered(&blobs, &root, &RenderLimits::default());
    assert_eq!(whole.json, json!(vec!["n".repeat(100); 50]));
    assert!(!whole.cut);

    let limits = RenderLimits {
        max_text_bytes: 250,
        ..RenderLimits::default()
    };
    let bounded = rendered(&blobs, &root, &limits);
    let mut expected = vec![json!("n".repeat(100)); 2];
    expected.resize(50, json!({"$truncated": "render_size"}));
    assert_eq!(bounded.json, json!(expected));
    assert!(bounded.cut);
    assert_eq!(bounded.incomplete, None, "a cut is not missing evidence");
}

#[test]
fn text_that_does_not_fit_is_cut_and_a_key_that_does_not_fit_ends_its_object() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), |b| {
        let note = text(b, &"n".repeat(100));
        let yes = text(b, "yes");
        map(b, &[("note", note), ("ok", yes)])
    }));
    let render = |max_text_bytes| {
        let limits = RenderLimits {
            max_text_bytes,
            ..RenderLimits::default()
        };
        rendered(&blobs, &root, &limits)
    };
    // `note`, `ok` and `yes` fit, so smaller text after the cut still shows.
    let cut = render(9);
    assert_eq!(
        cut.json,
        json!({"note": {"$truncated": "render_size"}, "ok": "yes"})
    );
    assert!(cut.cut);
    assert_eq!(
        render(5).json,
        json!({
            "$map": {"note": {"$truncated": "render_size"}},
            "$truncated": "render_size",
        })
    );
    assert_eq!(
        render(0).json,
        json!({"$map": {}, "$truncated": "render_size"})
    );
}

#[test]
fn objects_end_where_the_node_limit_is_reached() {
    let mut blobs = Blobs::default();
    let root = blobs.store(&capture(&mut cut(), |b| {
        let first = list(b, &[V::Int(1), V::Int(2), V::Int(3)]);
        let second = list(b, &[V::Int(4)]);
        let pair = map(b, &[("a", V::Int(5)), ("b", V::Int(6))]);
        list(b, &[first, second, pair])
    }));
    let render = |max_nodes| {
        let limits = RenderLimits {
            max_nodes,
            ..RenderLimits::default()
        };
        rendered(&blobs, &root, &limits)
    };
    let whole = render(10);
    assert_eq!(whole.json, json!([[1, 2, 3], [4], {"a": 5, "b": 6}]));
    assert!(!whole.cut);
    // Each object still open is marked once, and nothing after it is visited.
    let cut = render(4);
    assert_eq!(
        cut.json,
        json!({
            "$list": [{"$list": [1, 2], "$truncated": "render_size"}],
            "$truncated": "render_size",
        })
    );
    assert!(cut.cut);
    assert_eq!(
        render(9).json,
        json!([[1, 2, 3], [4], {"$map": {"a": 5}, "$truncated": "render_size"}])
    );
}

#[test]
fn a_child_is_read_only_as_what_it_holds() {
    let mut blobs = Blobs::default();
    let arguments = {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let start = b.value_start();
        b.push_value(V::Int(1));
        let slots = b.value_range(start);
        blobs.store(&b.finish_args(1, slots, &mut Shaper::default()))
    };
    let small = blobs.store(&capture(&mut Shaper::default(), |b| list(b, &[V::Int(1)])));
    let parent = |id: u8, children: Vec<CasId>, value: DecodedValue| {
        Arc::new(DecodedSnapshot {
            id: CasId::from_bytes([id; 16]),
            encoded_len: 0,
            children,
            root: DecodedRoot::Value(DecodedValue::Object(NodeId(0))),
            objects: vec![DecodedObject::Cell(value)],
        })
    };
    let child = ChildIndex(0);
    // Captured arguments are not a value, and the list has one object.
    let as_value = parent(1, vec![arguments.id], DecodedValue::External(child));
    let past_the_end = parent(
        2,
        vec![small.id],
        DecodedValue::ExternalNode {
            child,
            node: NodeId(1),
        },
    );
    // A root that is itself stored elsewhere would never end.
    let chained = parent(3, vec![as_value.id], DecodedValue::External(child));
    let relay = Arc::new(DecodedSnapshot {
        root: DecodedRoot::Value(DecodedValue::External(child)),
        ..(*as_value).clone()
    });
    blobs.0.insert(relay.id, Ok(relay));
    let broken = Unavailable::Blob("cas_corrupt");
    for root in [as_value, past_the_end, chained] {
        assert_eq!(at(&blobs, &root, &[]), Nav::Unavailable(broken));
        let cell = DecodedValue::Object(NodeId(0));
        let span = Span::load(&blobs, [(&root, &cell)], Examine::Structure, 16, u64::MAX);
        let DecodedObject::Cell(inner) = root.object(NodeId(0)) else {
            unreachable!()
        };
        match inner {
            DecodedValue::External(child) => {
                assert_eq!(span.root_of(&root, *child).err(), Some(broken));
            }
            DecodedValue::ExternalNode { child, node } => {
                assert_eq!(span.node_of(&root, *child, *node).err(), Some(broken));
            }
            other => panic!("expected a reference, found {other:?}"),
        }
    }
}

#[test]
fn a_value_in_several_files_reads_through_the_cas_directory() {
    use crate::cas::CasLimits;

    let directory = tempfile::tempdir().unwrap();
    let snapshot = capture(&mut cut(), invoice);
    let mut scratch = BlobScratch::default();
    for blob in snapshot.blobs() {
        let path = btel_file::cas_path(directory.path(), blob.id());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = Vec::new();
        blob.write(&mut scratch, &mut bytes).unwrap();
        assert_eq!(bytes.len() as u64, blob.encoded_len());
        std::fs::write(&path, bytes).unwrap();
    }
    let cas = CasStore::new(directory.path().to_path_buf(), CasLimits::default());
    let root = BlobSource::load(&cas, snapshot.root_id()).unwrap();
    let find = |segments: &[&str]| navigate(&cas, &root, Root::Value, None, &path(segments), 16);
    let limits = RenderLimits::default();
    let text = |nav: &Nav| to_scalar(&cas, nav, None, &limits).scalar;
    assert_eq!(text(&find(&["note"])), Scalar::Text("n".repeat(100)));
    assert_eq!(
        text(&find(&["order", "tags", "1"])),
        Scalar::Text("t".repeat(100))
    );

    let file_of = |segments: &[&str]| {
        let Nav::Value(found) = find(segments) else {
            panic!("expected a value at {segments:?}")
        };
        btel_file::cas_path(directory.path(), found.blob().id)
    };
    let (note, photo) = (file_of(&["note"]), file_of(&["order", "photo"]));
    std::fs::remove_file(note).unwrap();
    let mut bytes = std::fs::read(&photo).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(photo, bytes).unwrap();
    assert_eq!(
        find(&["note"]),
        Nav::Unavailable(Unavailable::Blob("cas_missing"))
    );
    assert_eq!(
        find(&["order", "photo"]),
        Nav::Unavailable(Unavailable::Blob("cas_id_mismatch"))
    );
    assert_eq!(text(&find(&["order", "sku"])), Scalar::Text("A-1".into()));
}
