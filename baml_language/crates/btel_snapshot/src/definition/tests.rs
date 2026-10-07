use baml_type::{Name, TypeName, typetag::TypeTag};

use super::*;
use crate::{
    BlobError, Carried, DecodeLimits, DecodedObject, DecodedRoot, Limits, OwnedType, Shaper,
    SnapshotPool, SnapshotValue, TypeIdentity, decode_blob,
};

type Template = TyTemplate<Head>;

fn declared(path: &str) -> DeclarationName {
    DeclarationName::Declared(TypeName::from_dotted_path(path))
}

fn anonymous(name: &str) -> DeclarationName {
    DeclarationName::Anonymous(Name::new(name))
}

fn field(name: &str, ty: Template) -> Field<Template> {
    Field {
        name: name.into(),
        ty,
        meta: Meta::default(),
        skip: false,
        stream_done: false,
        must_exist: false,
    }
}

fn class(name: DeclarationName, fields: Vec<Field<Template>>) -> Declaration<Template> {
    Declaration::Class(Class {
        name,
        type_params: 0,
        meta: Meta::default(),
        stream_done: false,
        fields,
    })
}

fn optional(ty: Template) -> Template {
    TyTemplate::Union(Box::new([ty, TyTemplate::Null]))
}

fn member(position: u32) -> Template {
    TyTemplate::Class(Head::Member(position), Box::new([]))
}

fn id(definitions: &[Definition]) -> [u8; 16] {
    definitions[0].group.id()
}

fn decode(definition: &Definition) -> crate::DecodedSnapshot {
    decode_blob(definition.group.bytes(), &DecodeLimits::default()).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    })
}

/// `Left { right: Right? }` and `Right { left: Left? }`, in the given order.
fn left_right(left_first: bool) -> Vec<Declaration<Template>> {
    let (left, right) = if left_first { (0, 1) } else { (1, 0) };
    let mut members = vec![
        class(
            declared("user.Left"),
            vec![field("right", optional(member(right)))],
        ),
        class(
            declared("user.Right"),
            vec![field("left", optional(member(left)))],
        ),
    ];
    if !left_first {
        members.reverse();
    }
    members
}

#[test]
fn a_group_hashes_the_same_in_any_order_it_is_found() {
    let found_left_first = group(&left_right(true));
    let found_right_first = group(&left_right(false));
    assert_eq!(id(&found_left_first), id(&found_right_first));
    // Each class keeps its place: Left was first in one input, second in
    // the other.
    assert_eq!(found_left_first[0].member, found_right_first[1].member);
    assert_eq!(found_left_first[1].member, found_right_first[0].member);
    assert_ne!(found_left_first[0].member, found_left_first[1].member);
    assert_eq!(
        found_left_first[0].group.bytes(),
        found_right_first[0].group.bytes()
    );
}

#[test]
fn different_definitions_hash_differently() {
    let base = || class(anonymous("Person"), vec![field("name", TyTemplate::String)]);
    let same = group(&[base()]);
    assert_eq!(id(&same), id(&group(&[base()])));
    let renamed_field = class(
        anonymous("Person"),
        vec![field("email", TyTemplate::String)],
    );
    let retyped = class(anonymous("Person"), vec![field("name", TyTemplate::Int)]);
    let renamed = class(anonymous("Human"), vec![field("name", TyTemplate::String)]);
    let mut described = base();
    let Declaration::Class(inner) = &mut described else {
        unreachable!()
    };
    inner.meta.description = Some("a person".into());
    let ids = [
        id(&same),
        id(&group(&[renamed_field])),
        id(&group(&[retyped])),
        id(&group(&[renamed])),
        id(&group(&[described])),
    ];
    for (at, a) in ids.iter().enumerate() {
        for b in &ids[at + 1..] {
            assert_ne!(a, b);
        }
    }
}

/// Members that only their references tell apart: every start has the
/// same digest, and the smallest whole encoding decides.
#[test]
fn indistinguishable_members_hash_the_same_whichever_starts() {
    // A cycle of three `Node { next: Node }`, the node at cycle position `k`
    // handed over at `order[k]`.
    let ring = |order: [u32; 3]| {
        let mut members = vec![None, None, None];
        for k in 0..3 {
            let next = order[(k + 1) % 3];
            members[order[k] as usize] =
                Some(class(anonymous("Node"), vec![field("next", member(next))]));
        }
        members.into_iter().map(Option::unwrap).collect::<Vec<_>>()
    };
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let ids: Vec<_> = orders
        .into_iter()
        .map(|order| id(&group(&ring(order))))
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]), "{ids:?}");
    // A different shape among equal members is a different group.
    let pair = vec![
        class(anonymous("Node"), vec![field("next", member(1))]),
        class(anonymous("Node"), vec![field("next", member(0))]),
    ];
    assert_ne!(id(&group(&pair)), ids[0]);
}

#[test]
fn a_group_names_the_groups_it_uses_as_children() {
    let resume = group(&[class(
        declared("user.Resume"),
        vec![field("name", TyTemplate::String)],
    )]);
    let holder = group(&[class(
        declared("user.Holder"),
        vec![
            field(
                "resume",
                TyTemplate::Class(
                    Head::Defined(declared("user.Resume"), resume[0].clone()),
                    Box::new([]),
                ),
            ),
            field(
                "again",
                TyTemplate::List(Box::new(TyTemplate::Class(
                    Head::Defined(declared("user.Resume"), resume[0].clone()),
                    Box::new([]),
                ))),
            ),
            field("param", TyTemplate::TypeArgRef(0)),
        ],
    )]);
    let children: Vec<_> = holder[0].group.children().iter().map(|c| c.id()).collect();
    assert_eq!(children, vec![id(&resume)]);
    let decoded = decode(&holder[0]);
    assert_eq!(
        decoded.children,
        vec![crate::CasId::from_bytes(id(&resume))]
    );
    let DecodedRoot::Definitions(members) = &decoded.root else {
        panic!("a definition group")
    };
    let Declaration::Class(class) = &members[0] else {
        panic!("a class")
    };
    assert_eq!(class.fields.len(), 3);
    let resume_ref = DefinitionRef::of(&resume[0]);
    assert!(matches!(
        class.fields[0].ty.decoded.as_deref(),
        Some(TyTemplate::Class(DefinitionHead::Defined(head), _)) if head.definition == resume_ref
    ));
    assert_eq!(
        class.fields[2].ty.decoded.as_deref(),
        Some(&TyTemplate::TypeArgRef(0))
    );
}

#[test]
fn a_definition_round_trips_with_its_metadata() {
    let meta = |text: &str| Meta {
        description: Some(format!("{text} description")),
        alias: Some(format!("{text} alias")),
        docstring: Some(format!("{text} docs")),
        attributes: vec![("check".into(), text.into()), ("other".into(), "x".into())],
    };
    let resume = Declaration::Class(Class {
        name: declared("user.Resume"),
        type_params: 2,
        meta: meta("class"),
        stream_done: true,
        fields: vec![Field {
            name: "age".into(),
            ty: optional(TyTemplate::Int),
            meta: meta("field"),
            skip: true,
            stream_done: true,
            must_exist: true,
        }],
    });
    let status = Declaration::Enum(Enum {
        name: declared("user.Status"),
        meta: meta("enum"),
        variants: vec![
            Variant {
                name: "Active".into(),
                meta: meta("variant"),
                skip: false,
            },
            Variant {
                name: "Gone".into(),
                meta: Meta::default(),
                skip: true,
            },
        ],
    });
    for declaration in [resume, status] {
        let made = group(std::slice::from_ref(&declaration));
        let DecodedRoot::Definitions(members) = decode(&made[0]).root else {
            panic!("a definition group")
        };
        let decoded = &members[0];
        match (&declaration, decoded) {
            (Declaration::Class(made), Declaration::Class(read)) => {
                assert_eq!(read.name.to_string(), made.name.to_string());
                assert_eq!(read.type_params, made.type_params);
                assert_eq!(read.meta, made.meta);
                assert_eq!(read.stream_done, made.stream_done);
                let (made, read) = (&made.fields[0], &read.fields[0]);
                assert_eq!(read.name, made.name);
                assert_eq!(read.meta, made.meta);
                assert_eq!(
                    (read.skip, read.stream_done, read.must_exist),
                    (true, true, true)
                );
                assert_eq!(
                    read.ty.decoded.as_deref(),
                    Some(&TyTemplate::Union(Box::new([
                        TyTemplate::Int,
                        TyTemplate::Null
                    ])))
                );
            }
            (Declaration::Enum(made), Declaration::Enum(read)) => {
                assert_eq!(read.name.to_string(), made.name.to_string());
                assert_eq!(read.meta, made.meta);
                assert_eq!(read.variants, made.variants);
            }
            _ => panic!("a different kind"),
        }
    }
}

/// The ID depends on the definition alone: the same in every run and
/// process, on every machine. Changing this value changes every recorded
/// definition's ID, which is a format change.
#[test]
fn definition_ids_are_pinned() {
    let made = group(&left_right(true));
    assert_eq!(hex(&id(&made)), PINNED_LEFT_RIGHT);
    let person = group(&[class(
        anonymous("Person"),
        vec![
            field("name", TyTemplate::String),
            field("age", TyTemplate::Int),
        ],
    )]);
    assert_eq!(hex(&id(&person)), PINNED_PERSON);
}
const PINNED_LEFT_RIGHT: &str = "444384790fde70b5ca1c9c20b24e9382";
const PINNED_PERSON: &str = "edf9df1b604a29f2ecdb31d6a00166df";

/// A definition blob with no children whose content is `content`, with
/// the ID it hashes to.
fn sealed(content: &[u8]) -> Vec<u8> {
    let mut h = Hasher::new(HashDomain::Blob);
    h.byte(RootTag::Definitions as u8);
    h.absorb(content);
    h.size(0);
    h.size(0);
    let mut bytes = BLOB_MAGIC.to_vec();
    bytes.extend_from_slice(&BLOB_VERSION.to_le_bytes());
    bytes.extend_from_slice(&h.finish().0);
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(RootTag::Definitions as u8);
    bytes.extend_from_slice(content);
    bytes
}

#[test]
fn a_tampered_definition_is_rejected() {
    let made = group(&left_right(true));
    let mut bytes = made[0].group.bytes().to_vec();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert!(matches!(
        decode_blob(&bytes, &DecodeLimits::default()),
        Err(BlobError::IdMismatch { .. })
    ));

    // `Lonely { me: Lonely }`, resealed with a member position past the
    // group: invalid, though its ID matches.
    let lonely = group(&[class(anonymous("Lonely"), vec![field("me", member(0))])]);
    let header = 8 + 4 + 16 + 4 + 4 + 1;
    let content = &lonely[0].group.bytes()[header..];
    assert_eq!(sealed(content), lonely[0].group.bytes());
    // After the position: the head's empty arguments, the field's
    // metadata (three absent strings, no attributes) and its three flags.
    let at = content.len() - (4 + 4 + (3 + 4) + 3);
    let mut forged = content.to_vec();
    forged[at..at + 4].copy_from_slice(&7_u32.to_le_bytes());
    assert!(matches!(
        decode_blob(&sealed(&forged), &DecodeLimits::default()),
        Err(BlobError::Invalid(reason)) if reason.contains("out of range")
    ));
}

/// A capture naming `person` as a declaration and a type head, with
/// `carried` saying which groups its stream carried already, or naming a
/// class by its runtime tag when `person` is `None`.
fn person_capture(
    pool: &SnapshotPool,
    tag: i64,
    person: Option<&Definition>,
    carried: &mut Carried,
) -> crate::Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let definition = person.map(|person| b.leaves().define(person, carried));
    let declaration = b.declaration(
        &anonymous("Person"),
        TypeTag::from_i64(tag),
        false,
        definition,
    );
    let declaration = b.leaves().object(declaration).unwrap();
    let ty = match person {
        Some(person) => OwnedType::Class(
            TypeIdentity::Defined(b.leaves().define(person, carried)),
            Box::new([]),
        ),
        None => OwnedType::Class(
            TypeIdentity::Resolved(baml_type::TaggedTypeName::new(
                TypeTag::from_i64(tag),
                anonymous("Person"),
            )),
            Box::new([]),
        ),
    };
    let ty = b.leaves().ty(ty);
    let instance = b.instance(
        declaration,
        [],
        [("name", "ann")].into_iter(),
        |leaves, (key, value)| (key.into(), leaves.string_value(&value.into())),
    );
    let instance = b.leaves().object(instance).unwrap();
    let args = b.arguments(
        [SnapshotValue::Object(instance), SnapshotValue::Type(ty)].into_iter(),
        |_, value| value,
    );
    b.finish(args, &mut Shaper::default())
}

fn root_bytes(snapshot: &crate::Snapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    snapshot
        .root_blob()
        .write(&mut crate::BlobScratch::default(), &mut bytes)
        .unwrap();
    bytes
}

/// The first capture of a stream to name a group carries it, before the
/// capture's own blob; later ones name it by ID alone, and their blobs are
/// the same. A stream's carried set is its own.
#[test]
fn a_stream_carries_a_group_once_and_names_it_after() {
    let _carrying = carrying();
    let pool = SnapshotPool::new(4, Limits::default());
    let person = group(&[class(
        anonymous("Person"),
        vec![field("name", TyTemplate::String)],
    )]);
    let group_id = crate::CasId::from_bytes(id(&person));
    let mut stream = Carried::default();
    let first = person_capture(&pool, 1, Some(&person[0]), &mut stream);
    let ids: Vec<_> = first.blobs().map(|blob| blob.id()).collect();
    assert_eq!(ids, vec![group_id, first.root_id()]);
    let mut group_bytes = Vec::new();
    first
        .blobs()
        .next()
        .unwrap()
        .write(&mut crate::BlobScratch::default(), &mut group_bytes)
        .unwrap();
    assert_eq!(group_bytes, person[0].group.bytes());

    let later = person_capture(&pool, 1, Some(&person[0]), &mut stream);
    let ids: Vec<_> = later.blobs().map(|blob| blob.id()).collect();
    assert_eq!(ids, vec![later.root_id()], "named by ID alone");
    assert_eq!(later.root_id(), first.root_id());
    assert_eq!(root_bytes(&later), root_bytes(&first));
    assert_eq!(later.root_blob().named(), [group_id]);

    let decoded = decode_blob(&root_bytes(&later), &DecodeLimits::default()).unwrap();
    assert_eq!(decoded.children, vec![group_id]);
    assert!(decoded.objects.iter().any(|object| matches!(
        object,
        DecodedObject::Declaration { tag: None, definition: Some(definition), .. }
            if *definition == DefinitionRef::of(&person[0])
    )));

    let other_stream = person_capture(&pool, 1, Some(&person[0]), &mut Carried::default());
    assert_eq!(
        other_stream.blobs().len(),
        2,
        "another stream carries it too"
    );
}

/// Tests that count what a stream carries hold this, because a possible loss
/// makes every stream in the process carry again.
fn carrying() -> std::sync::MutexGuard<'static, ()> {
    static CARRYING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CARRYING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// After a writer may have lost a capture, a stream carries a group it
/// carried before again, with its next capture that names it, and then names
/// it by ID alone.
#[test]
fn a_possible_loss_makes_streams_carry_again() {
    let _carrying = carrying();
    let pool = SnapshotPool::new(4, Limits::default());
    let person = group(&[class(
        anonymous("Person"),
        vec![field("name", TyTemplate::String)],
    )]);
    let mut stream = Carried::default();
    let mut blobs = || {
        person_capture(&pool, 1, Some(&person[0]), &mut stream)
            .blobs()
            .len()
    };
    assert_eq!(blobs(), 2, "carried");
    assert_eq!(blobs(), 1, "named by ID");
    crate::forget_carried();
    assert_eq!(blobs(), 2, "carried again");
    assert_eq!(blobs(), 1, "named by ID again");
}

/// A group carried for the first time brings the groups it names that the
/// stream has not carried, each before the groups that name it.
#[test]
fn carrying_a_group_carries_what_it_names_first() {
    let _carrying = carrying();
    let pool = SnapshotPool::new(4, Limits::default());
    let resume = group(&[class(
        declared("user.Resume"),
        vec![field("name", TyTemplate::String)],
    )]);
    let holder = group(&[class(
        declared("user.Holder"),
        vec![field(
            "resume",
            TyTemplate::Class(
                Head::Defined(declared("user.Resume"), resume[0].clone()),
                Box::new([]),
            ),
        )],
    )]);
    let capture = |carried: &mut Carried| {
        let mut b = pool.try_acquire().unwrap();
        let holder = b.leaves().define(&holder[0], carried);
        let ty = b.leaves().ty(OwnedType::Class(
            TypeIdentity::Defined(holder),
            Box::new([]),
        ));
        b.finish(SnapshotValue::Type(ty), &mut Shaper::default())
    };
    let mut fresh = Carried::default();
    let ids: Vec<_> = capture(&mut fresh)
        .blobs()
        .map(|blob| blob.id().as_bytes().to_owned())
        .collect();
    assert_eq!(&ids[..2], [id(&resume), id(&holder)]);
    let mut knows_resume = Carried::default();
    let _ = pool
        .try_acquire()
        .unwrap()
        .leaves()
        .define(&resume[0], &mut knows_resume);
    let ids: Vec<_> = capture(&mut knows_resume)
        .blobs()
        .map(|blob| blob.id().as_bytes().to_owned())
        .collect();
    assert_eq!(&ids[..1], [id(&holder)]);
    assert_eq!(ids.len(), 2);
}

/// Classes that differ only in their runtime tags capture identically
/// once they are named by their definitions, and not before.
#[test]
fn identical_runtime_classes_capture_identically() {
    let pool = SnapshotPool::new(4, Limits::default());
    let person = group(&[class(
        anonymous("Person"),
        vec![field("name", TyTemplate::String)],
    )]);
    let mut carried = Carried::default();
    assert_eq!(
        person_capture(&pool, 1, Some(&person[0]), &mut carried).root_id(),
        person_capture(&pool, 2, Some(&person[0]), &mut carried).root_id()
    );
    assert_ne!(
        person_capture(&pool, 1, None, &mut carried).root_id(),
        person_capture(&pool, 2, None, &mut carried).root_id()
    );
}

/// Members a group cannot tell apart get the same position, whichever was
/// handed over first, so a definition that names one of them is the same
/// in every process.
#[test]
fn indistinguishable_members_get_one_position_in_any_order() {
    // `Node { next: Node }` twice, pointing at each other: either can start.
    let pair = || {
        vec![
            class(anonymous("Node"), vec![field("next", member(1))]),
            class(anonymous("Node"), vec![field("next", member(0))]),
        ]
    };
    let made = group(&pair());
    assert_eq!((made[0].member, made[1].member), (0, 0));
    // A ring of three identical nodes, handed over in each of the six
    // orders: every member is at position 0, so a class that names any of
    // them is the same definition.
    let ring = |order: [u32; 3]| {
        let mut members = vec![None, None, None];
        for k in 0..3 {
            let next = order[(k + 1) % 3];
            members[order[k] as usize] =
                Some(class(anonymous("Node"), vec![field("next", member(next))]));
        }
        members.into_iter().map(Option::unwrap).collect::<Vec<_>>()
    };
    let holder = |node: Definition| {
        id(&group(&[class(
            anonymous("Holder"),
            vec![field(
                "node",
                TyTemplate::Class(Head::Defined(anonymous("Node"), node), Box::new([])),
            )],
        )]))
    };
    let mut holders = Vec::new();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let made = group(&ring(order));
        assert!(made.iter().all(|node| node.member == 0));
        holders.extend(made.into_iter().map(holder));
    }
    assert!(
        holders.windows(2).all(|pair| pair[0] == pair[1]),
        "{holders:?}"
    );
    // Members that can be told apart keep distinct positions.
    let made = group(&left_right(true));
    assert_ne!(made[0].member, made[1].member);
}

/// A leaf hashed in one call has the digest of hashing it piece by piece,
/// which is how a reader checks it.
#[test]
fn type_leaves_hash_as_a_reader_checks_them() {
    let definition = group(&[class(anonymous("Person"), vec![])]).remove(0);
    let types = [
        OwnedType::String,
        OwnedType::Type,
        OwnedType::List(Box::new(OwnedType::Int)),
        OwnedType::Union(Box::new([
            OwnedType::Class(
                TypeIdentity::Defined(DefinitionRef::of(&definition)),
                Box::new([]),
            ),
            OwnedType::Null,
        ])),
        OwnedType::Class(
            TypeIdentity::Resolved(baml_type::TaggedTypeName::new(
                TypeTag::from_i64(7),
                declared("user.Resume"),
            )),
            Box::new([]),
        ),
    ];
    for ty in types {
        let mut streamed = Hasher::new(HashDomain::Type);
        let len = streamed.borsh(&ty);
        // Twice: a type without parts is made once and remembered.
        for _ in 0..2 {
            let leaf = crate::hash::ty(&ty);
            assert_eq!(leaf.digest, streamed.finish(), "{ty:?}");
            assert_eq!(leaf.encoded_len as usize, len);
        }
    }
}
