//! Validates the on-demand range path: `semantic_tokens_in_range(range)` must
//! equal the full `semantic_tokens()` result filtered to that range, for every
//! sub-range — the rust-analyzer `highlight_range` correctness property.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_lsp2_actions::tokens::{SemanticTokenType, semantic_tokens, semantic_tokens_in_range};
    use baml_project::ProjectDatabase;
    use text_size::{TextRange, TextSize};

    fn check(src: &str) {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("."));
        let file = db.add_or_update_file(Path::new("range.baml"), src);

        let full = semantic_tokens(&db, file);

        // Every contiguous window of the full token stream defines a range; the
        // range query over that window must reproduce exactly the windowed tokens.
        for i in 0..full.len() {
            for j in i..full.len() {
                let range = full[i].range.cover(full[j].range);
                let expected: Vec<_> = full
                    .iter()
                    .filter(|t| range.intersect(t.range).is_some())
                    .cloned()
                    .collect();
                let actual =
                    semantic_tokens_in_range(&db, file, range.start().into(), range.end().into());
                assert_eq!(
                    actual, expected,
                    "range {range:?} mismatch\n  expected: {expected:?}\n  actual:   {actual:?}"
                );
            }
        }
    }

    #[test]
    fn range_equals_filtered_full() {
        check(
            r#"class Box<T> { v: T }
interface Greeter { function greet(self) -> string }
function use_it(b: Box<int>) -> int {
  let xs = [1, 2, 3];
  let f = spawn with baml.spawn.options() { baml.sys.sleep(baml.time.Duration.from_milliseconds(5n)); 1 };
  let g = b.v + (await f);
  g
}
"#,
        );
    }

    #[test]
    fn qualified_type_tokens_are_in_document_order() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("."));
        db.add_or_update_file(Path::new("ns_foo/types.baml"), "class Bar {}\n");
        let source = "class Qualified { value: root.foo.Bar }\n";
        let file = db.add_or_update_file(Path::new("ordered.baml"), source);

        let tokens = semantic_tokens(&db, file);
        assert!(
            tokens
                .windows(2)
                .all(|pair| pair[0].range.start() <= pair[1].range.start()),
            "semantic token ranges must be ordered for LSP delta encoding: {tokens:?}"
        );

        for (offset, _) in source.match_indices('.') {
            let range = TextRange::at(
                TextSize::new(u32::try_from(offset).unwrap()),
                TextSize::new(1),
            );
            let token = tokens
                .iter()
                .find(|token| token.range == range)
                .unwrap_or_else(|| {
                    panic!("missing semantic token for namespace separator {range:?}")
                });
            assert_eq!(token.token_type, SemanticTokenType::Namespace);
        }
    }
}
