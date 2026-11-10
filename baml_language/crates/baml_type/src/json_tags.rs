//! How a union with a `@json.fallback` member tells its members apart.
//!
//! A member's literal fields are its tags. The tree reads one key at a time:
//! first the keys the fallback class names, which every member has, then
//! whatever key still tells the remaining members apart. A value no member
//! has is the fallback's. A member reached is the one, so a literal the tree
//! never read, or any other field, fails as that member rather than falling
//! through to the fallback.
//!
//! The compiler builds it to reject a union it can't tell apart, the engine
//! to pick a member, and the Rust generator to write the same choice in
//! generated code, so the three can't disagree.

/// The choice a union makes for a JSON object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagTree<L> {
    /// This member, an index into the members the tree was built from.
    Member(usize),
    /// Read `key`: each value leads on, and any other value of the same JSON
    /// kind is the fallback.
    Read {
        key: String,
        arms: Vec<(L, TagTree<L>)>,
    },
}

/// Why a union has no [`TagTree`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Untagged {
    /// The fallback class has no literal field to name the tag.
    NoTag,
    /// This member lacks the tag the fallback class names.
    Member(usize),
    /// No key tells these two members apart.
    Overlap(usize, usize),
}

/// The tree for `members`, each its literal fields as JSON keys and values,
/// given the literal fields of the union's fallback class.
pub fn tag_tree<L: Clone + PartialEq>(
    members: &[Vec<(String, L)>],
    fallback: &[(String, L)],
) -> Result<TagTree<L>, Untagged> {
    let (tag, _) = fallback.first().ok_or(Untagged::NoTag)?;
    if let Some(member) = members
        .iter()
        .position(|member| value_of(member, tag).is_none())
    {
        return Err(Untagged::Member(member));
    }
    let preferred: Vec<&str> = fallback.iter().map(|(key, _)| key.as_str()).collect();
    let everyone: Vec<usize> = (0..members.len()).collect();
    read(members, &everyone, tag, &preferred)
}

/// Split `candidates` by `key`.
fn read<L: Clone + PartialEq>(
    members: &[Vec<(String, L)>],
    candidates: &[usize],
    key: &str,
    preferred: &[&str],
) -> Result<TagTree<L>, Untagged> {
    let mut groups: Vec<(L, Vec<usize>)> = Vec::new();
    for &candidate in candidates {
        let Some(value) = value_of(&members[candidate], key) else {
            return Err(Untagged::Member(candidate));
        };
        match groups.iter_mut().find(|(seen, _)| seen == value) {
            Some((_, group)) => group.push(candidate),
            None => groups.push((value.clone(), vec![candidate])),
        }
    }
    let arms = groups
        .into_iter()
        .map(|(value, group)| Ok((value, split(members, &group, preferred)?)))
        .collect::<Result<_, Untagged>>()?;
    Ok(TagTree::Read {
        key: key.to_string(),
        arms,
    })
}

/// The tree under one tag value: its only member, or the next key that tells
/// the members holding that value apart.
fn split<L: Clone + PartialEq>(
    members: &[Vec<(String, L)>],
    candidates: &[usize],
    preferred: &[&str],
) -> Result<TagTree<L>, Untagged> {
    let [first, rest @ ..] = candidates else {
        unreachable!("a tag value has a member")
    };
    let Some(&other) = rest.first() else {
        return Ok(TagTree::Member(*first));
    };
    let declared = members[*first].iter().map(|(key, _)| key.as_str());
    let key = preferred.iter().copied().chain(declared).find(|key| {
        let mut values = candidates
            .iter()
            .map(|&candidate| value_of(&members[candidate], key));
        let Some(Some(value)) = values.next() else {
            return false;
        };
        let mut distinct = false;
        for other in values {
            match other {
                Some(other) => distinct |= other != value,
                None => return false,
            }
        }
        distinct
    });
    match key {
        Some(key) => read(members, candidates, key, preferred),
        None => Err(Untagged::Overlap(*first, other)),
    }
}

fn value_of<'a, L>(member: &'a [(String, L)], key: &str) -> Option<&'a L> {
    member
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(literals: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
        literals
            .iter()
            .map(|(key, value)| ((*key).to_string(), *value))
            .collect()
    }

    fn read_of(
        key: &str,
        arms: Vec<(&'static str, TagTree<&'static str>)>,
    ) -> TagTree<&'static str> {
        TagTree::Read {
            key: key.to_string(),
            arms,
        }
    }

    #[test]
    fn a_tag_decides_and_other_literals_are_left_to_the_member() {
        // `role` sorts before `type`, and is not what the fallback names.
        let members = [
            member(&[("role", "admin"), ("type", "manager")]),
            member(&[("role", "user"), ("type", "workspace")]),
        ];
        assert_eq!(
            tag_tree(&members, &member(&[("type", "UNKNOWN")])),
            Ok(read_of(
                "type",
                vec![
                    ("manager", TagTree::Member(0)),
                    ("workspace", TagTree::Member(1)),
                ]
            ))
        );
    }

    #[test]
    fn members_sharing_a_tag_are_told_apart_by_the_next_key() {
        let members = [
            member(&[("service", "s3"), ("type", "storage")]),
            member(&[("service", "gcs"), ("type", "storage")]),
            member(&[("type", "ai")]),
        ];
        assert_eq!(
            tag_tree(&members, &member(&[("type", "UNKNOWN")])),
            Ok(read_of(
                "type",
                vec![
                    (
                        "storage",
                        read_of(
                            "service",
                            vec![("s3", TagTree::Member(0)), ("gcs", TagTree::Member(1))]
                        )
                    ),
                    ("ai", TagTree::Member(2)),
                ]
            ))
        );
    }

    #[test]
    fn one_member_is_still_read_so_the_fallback_is_reachable() {
        assert_eq!(
            tag_tree(&[member(&[("type", "a")])], &member(&[("type", "UNKNOWN")])),
            Ok(read_of("type", vec![("a", TagTree::Member(0))]))
        );
    }

    #[test]
    fn a_union_that_cannot_be_told_apart_has_no_tree() {
        let fallback = member(&[("type", "UNKNOWN")]);
        assert_eq!(
            tag_tree(&[member(&[("name", "x")])], &fallback),
            Err(Untagged::Member(0))
        );
        assert_eq!(
            tag_tree(
                &[member(&[("type", "a")]), member(&[("type", "a")])],
                &fallback
            ),
            Err(Untagged::Overlap(0, 1))
        );
        assert_eq!(
            tag_tree(
                &[
                    member(&[("type", "a"), ("kind", "k")]),
                    member(&[("type", "a")])
                ],
                &fallback
            ),
            Err(Untagged::Overlap(0, 1))
        );
        assert_eq!(
            tag_tree(&[member(&[("type", "a")])], &[]),
            Err(Untagged::NoTag)
        );
    }
}
