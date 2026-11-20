//! Every `.baml` file the repo owns, the formatter's own cases included, has
//! each of its plain comments attached exactly once, to a node directly
//! beside it: a leading comment's next owned token is the node's, a trailing
//! comment's previous owned token is the node's (at most a list separator
//! between), and a dangling comment follows the very token it names. The
//! attachment rule checked on real inputs rather than only on unit cases,
//! parse errors included.

use std::{collections::BTreeSet, path::Path};

use baml_fmt::comments::{CommentKind, Comments, Placement, Position};
use baml_syntax::{
    index::TreeIndex,
    nodes::FileTree,
    token::{TokenIdx, TokenKind},
    tree::TreeWalk as _,
};

const ROOTS: [&str; 5] = [
    "../baml_tests/baml_src",
    "../baml_tests/projects",
    "../baml_builtins2/baml_std",
    "../../tools/type_quiz",
    "tests/cases",
];

/// The nearest token before `token` that a node owns.
fn owned_before(index: &TreeIndex, token: TokenIdx) -> Option<TokenIdx> {
    (0..token.0)
        .rev()
        .map(TokenIdx)
        .find(|&t| index.token_owner(t).is_some())
}

/// The nearest token after `token` that a node owns.
fn owned_after(tree: &FileTree, index: &TreeIndex, token: TokenIdx) -> Option<TokenIdx> {
    let len = u32::try_from(tree.tokens().len()).unwrap_or_else(|_| unreachable!("bounded"));
    (token.0 + 1..len)
        .map(TokenIdx)
        .find(|&t| index.token_owner(t).is_some())
}

#[test]
fn every_corpus_comment_is_attached_once_to_a_node_beside_it() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = 0usize;
    let mut comments_total = 0usize;
    for root in ROOTS {
        let root = crate_dir.join(root);
        for entry in walkdir::WalkDir::new(&root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        {
            let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
            if entry.path().extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            let path = entry.path();
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            let parsed = baml_parser::parse(&text);
            let tree = &parsed.tree;
            let index = TreeIndex::build(tree);
            let comments = Comments::from_tree(tree, &index);

            let tokens = tree.tokens();
            let free_comments: BTreeSet<TokenIdx> = tokens
                .iter()
                .filter(|&t| {
                    matches!(
                        tokens.kind(t),
                        TokenKind::LineComment
                            | TokenKind::HeaderComment
                            | TokenKind::BlockComment
                            | TokenKind::UnclosedBlockComment
                    ) && index.token_owner(t).is_none()
                })
                .collect();
            let attached: BTreeSet<TokenIdx> = comments.all().iter().map(|c| c.token).collect();
            assert_eq!(
                attached.len(),
                comments.all().len(),
                "{}: no comment twice",
                path.display()
            );
            assert_eq!(
                attached,
                free_comments,
                "{}: every free comment attached",
                path.display()
            );

            for comment in comments.all() {
                let span = tree.arenas().span(comment.node);
                let within = |token: Option<TokenIdx>| {
                    token.is_some_and(|t| span.start() <= t && t < span.end())
                };
                let prev = owned_before(&index, comment.token);
                let sound = match comment.position {
                    Position::Leading => within(owned_after(tree, &index, comment.token)),
                    Position::Trailing => {
                        within(prev)
                            || prev.is_some_and(|separator| {
                                matches!(
                                    tokens.kind(separator),
                                    TokenKind::Comma | TokenKind::Semicolon | TokenKind::Pipe
                                ) && within(owned_before(&index, separator))
                            })
                    }
                    // Directly after the token, or on a line that ends before
                    // the node's terminator or before an item's `,`/`;` (a
                    // line comment ending the item's line, or a comment on a
                    // line of its own), which the comment is moved past.
                    Position::Dangling { after: Some(token) } => {
                        index.token_owner(token) == Some(comment.node)
                            && (prev == Some(token)
                                || (owned_after(tree, &index, comment.token) == Some(token)
                                    && matches!(
                                        tokens.kind(token),
                                        TokenKind::Comma | TokenKind::Semicolon
                                    )
                                    && (TokenIdx(token.0 + 1) == span.end()
                                        || comment.placement == Placement::OwnLine
                                        || (comment.placement == Placement::EndOfLine
                                            && comment.kind == CommentKind::Line))))
                    }
                    Position::Dangling { after: None } => {
                        prev.is_none() && comment.node == tree.root_id()
                    }
                };
                assert!(
                    sound,
                    "{}: {:?} {:?} of {} at {:?}, which spans {:?}",
                    path.display(),
                    comment.text(tree),
                    comment.position,
                    tree.arenas().kind_name(comment.node),
                    comment.token,
                    span
                );
            }
            files += 1;
            comments_total += comments.all().len();
        }
    }
    assert!(files > 500, "the corpus roots were found ({files} files)");
    assert!(
        comments_total > 1000,
        "the corpus has comments ({comments_total})"
    );
}
