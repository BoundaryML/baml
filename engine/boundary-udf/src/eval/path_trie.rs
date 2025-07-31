//! Trie for variable access paths (`foo.bar.baz`), used to group accesses such that when
//! zeroing one we can immediately visit all of the subsequent undefined accesses that should
//! all be zero too.

pub fn zero_undefined_values(
    trie: &PathTrie,
    tracked_value: &mut serde_json::Value,
    zeroed_lists: &mut [Vec<String>],
) {
    // DFS the path trie, in two modes:
    // - tracking mode: starting with the existing map, on each node visited (each `PathTrie` has
    // multiple path-compressed nodes):
    //      - try to interpret the current value as a map. If it fails (e.g it's a number), the
    //      expression will fail when interpreting and we will not consider it for zeroing. Stop DFS
    //      here.
    //      - try to access the map with this node as a key. If it fails, we're going to consider all
    //      the paths as zero. Switch this node to zeroing mode.
    // - zeroing mode: DFS all nodes.
    //      - if node is terminal, add its path to all registered returns.
    //      - if node does not have any children, define it as zero value on the map.
    //      - if node has children, define it as a map. Keep the map reference to track it.
    //
    // Due to Rust restrictions on &mut pointers, the algorithm will use a recursive DFS.
    dfs_tracking(trie, tracked_value, zeroed_lists)
}

fn dfs_tracking(
    trie: &PathTrie,
    tracked_value: &mut serde_json::Value,
    zeroed_lists: &mut [Vec<String>],
) {
    visit_pieces(
        trie.partial_path.iter().copied().enumerate(),
        trie,
        tracked_value,
        zeroed_lists,
    );

    // NOTE: (Jesus) tail-recursive, because I couldn't get the for-loop version to work.
    // It complained about `as_map` being used in the part that bails out, i.e:
    // ```rs
    // let Some(next) = as_map.get_mut(piece) else {
    // // says it's borrowed here
    //    return dfs_transition_to_zeroing(...);
    // }
    fn visit_pieces<'s, 'l>(
        mut pieces: impl Iterator<Item = (usize, &'l str)>,
        trie: &'l PathTrie<'l>,
        tracked_value: &'s mut serde_json::Value,
        zeroed_lists: &'l mut [Vec<String>],
    ) {
        match pieces.next() {
            None => {
                // continue DFS
                for child in &trie.children {
                    dfs_tracking(child, tracked_value, zeroed_lists);
                }
            }
            Some((piece_index, piece)) => {
                let Some(as_map) = tracked_value.as_object_mut() else {
                    return;
                };

                match as_map.get_mut(piece) {
                    Some(next) => visit_pieces(pieces, trie, next, zeroed_lists),
                    None => {
                        dfs_transition_to_zeroing(trie, piece_index, tracked_value, zeroed_lists)
                    }
                }
            }
        }
    }
}

pub fn dfs_transition_to_zeroing(
    trie: &PathTrie,
    piece_index: usize,
    mut tracked_value: &mut serde_json::Value,
    zeroed_lists: &mut [Vec<String>],
) {
    let last_piece_with_children = if trie.children.is_empty() {
        trie.partial_path.len() - 1
    } else {
        trie.partial_path.len()
    };

    // problem (documented in src/eval.rs, README.md):
    // - if I make a map, then the ones that are using it as `if` are wrong (it yields
    // non-zero).
    // - if I make a value, then the ones using as a map are wrong, and I cauld have it in
    // the trie as terminal because there's a usage within an `if` (e.g `if hello.world`)
    //
    // -> node has children -> it's used elsewhere as a map.
    // If it is used within `if`, then there's a problem. Can I just use the AST to find
    // about it? -> not doing that. Annotated at the top what the problem looks like.
    // -> node has no children -> it's safe to set to zero

    for &piece in &trie.partial_path[piece_index..last_piece_with_children] {
        // non-terminal and has at least 1 child.

        let object = tracked_value.as_object_mut().unwrap();

        object.insert(piece.into(), serde_json::Map::new().into());

        tracked_value = object.get_mut(piece).unwrap();
    }

    if let Some(terminal) = trie.terminal_full_path.as_ref() {
        // last needs to be zero.
        tracked_value.as_object_mut().unwrap().insert(
            trie.partial_path.last().copied().unwrap().into(),
            0f64.into(),
        );

        for &index in &terminal.results {
            zeroed_lists[index.0].push(terminal.full_path.into());
        }
    }

    for child in &trie.children {
        dfs_zeroing(child, tracked_value, zeroed_lists);
    }
}

#[inline]
fn dfs_zeroing(
    trie: &PathTrie,
    tracked_value: &mut serde_json::Value,
    zeroed_lists: &mut [Vec<String>],
) {
    dfs_transition_to_zeroing(trie, 0, tracked_value, zeroed_lists)
}

/// Index from iteration order (which is consistent due to IndexMap) for a certain
/// return. Used to register it into the trie.
#[derive(Clone, Copy)]
pub struct ReturnIterationOrder(pub usize);

struct TrieTerminal<'a> {
    full_path: &'a str,
    /// The results that are registered for this terminal. If a variable path is found to be zero,
    /// then all of these should have said path added to their "missing value" list.
    results: Vec<ReturnIterationOrder>,
}

#[derive(Default)]
pub struct PathTrie<'a> {
    partial_path: Vec<&'a str>,
    children: Vec<PathTrie<'a>>,
    terminal_full_path: Option<TrieTerminal<'a>>,
}

impl<'src> PathTrie<'src> {
    pub fn insert(&mut self, path: &'src str, ret_handle: ReturnIterationOrder) {
        insert_into_trie_recursive(self, path, path.split('.'), ret_handle)
    }
}

// using a tail-recursive approach because that plays better with the borrow checker.
fn insert_into_trie_recursive<'src>(
    root: &mut PathTrie<'src>,
    path: &'src str,
    mut path_it: impl Iterator<Item = &'src str>,
    ret_handle: ReturnIterationOrder,
) {
    // assume everything until `root` is matched.
    let Some(first) = path_it.next() else {
        root.terminal_full_path = Some(match root.terminal_full_path.take() {
            Some(mut terminal) => {
                terminal.results.push(ret_handle);
                terminal
            }
            None => TrieTerminal {
                full_path: path,
                results: vec![ret_handle],
            },
        });

        return;
    };

    // find a child that matches.
    let matching_child = root
        .children
        .iter_mut()
        .find(|child| child.partial_path[0] == first);

    match matching_child {
        None => {
            // found no children -> insert into the trie.
            root.children.push(PathTrie {
                partial_path: [first].into_iter().chain(path_it).collect(),
                children: Vec::new(),
                terminal_full_path: Some(TrieTerminal {
                    full_path: path,
                    results: vec![ret_handle],
                }),
            });
            return;
        }
        Some(child) => {
            // match as much of the path as possible.
            for index in 1..child.partial_path.len() {
                match path_it.next() {
                    None => {
                        // cut the child abcd -> abc {d}, with abc terminal.
                        let partial_path = child.partial_path.drain(0..index).collect();

                        let cutoff_child = std::mem::replace(
                            child,
                            PathTrie {
                                partial_path,
                                children: Vec::with_capacity(1),
                                terminal_full_path: Some(TrieTerminal {
                                    full_path: path,
                                    results: vec![ret_handle],
                                }),
                            },
                        );

                        // child is now the branch that we inserted.
                        child.children.push(cutoff_child);
                        return;
                    }
                    Some(piece) => {
                        if piece == child.partial_path[index] {
                            // matched, go to next.
                            continue;
                        }

                        // cut the child abcd -> abc {d e}, with abc non-terminal

                        let partial_path = child.partial_path.drain(0..index).collect();
                        let cutoff_child = std::mem::replace(
                            child,
                            PathTrie {
                                partial_path,
                                children: Vec::with_capacity(2),
                                terminal_full_path: None,
                            },
                        );

                        let branch = child;
                        branch.children.push(cutoff_child);
                        branch.children.push(PathTrie {
                            partial_path: [piece].into_iter().chain(path_it).collect(),
                            children: Vec::new(),
                            terminal_full_path: Some(TrieTerminal {
                                full_path: path,
                                results: vec![ret_handle],
                            }),
                        });
                        return;
                    }
                }
            }

            // full path has been matched.
            insert_into_trie_recursive(child, path, path_it, ret_handle)
        }
    }
}
