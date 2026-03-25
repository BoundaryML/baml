#!/usr/bin/env python3
"""Fix remaining Ty sites by processing files directly with context-aware regex.

This script processes each file and adds TyAttr::default() to Ty:: constructions
and _ to Ty:: pattern matches, handling both single-line and multi-line cases.
"""

import re
import os

BASE = "/Users/sam/baml2/baml_language/crates/baml_compiler2_tir/src"

# Tuple variants and their expected arg counts (old -> new)
# old_args -> new_args
TUPLE_VARIANTS = {
    'Class': (1, 2),
    'Enum': (1, 2),
    'EnumVariant': (2, 3),
    'TypeAlias': (1, 2),
    'Primitive': (1, 2),
    'List': (1, 2),
    'Map': (2, 3),
    'Union': (1, 2),
    'Optional': (1, 2),
    'Literal': (2, 3),
    'EvolvingList': (1, 2),
    'EvolvingMap': (2, 3),
    'TypeVar': (1, 2),
}

def find_matching_close_in_content(content, pos, open_ch='(', close_ch=')'):
    """Find matching closing char in content string starting at pos (which should be the open char)."""
    depth = 0
    i = pos
    in_string = False
    while i < len(content):
        ch = content[i]
        if ch == '"' and (i == 0 or content[i-1] != '\\'):
            in_string = not in_string
        elif not in_string:
            if ch == open_ch:
                depth += 1
            elif ch == close_ch:
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return None

def count_top_level_commas(content, start, end):
    """Count commas at depth 0 between start and end positions."""
    depth = 0
    count = 0
    in_string = False
    for i in range(start, end):
        ch = content[i]
        if ch == '"' and (i == 0 or content[i-1] != '\\'):
            in_string = not in_string
        elif not in_string:
            if ch in '([{':
                depth += 1
            elif ch in ')]}':
                depth -= 1
            elif ch == ',' and depth == 0:
                count += 1
    return count

def is_pattern_context(content, pos):
    """Heuristic: is position 'pos' in a pattern context?

    Look backwards for match arm indicators:
    - We're between a match { and its =>
    - We're inside matches!()
    - We're after if let
    """
    # Look backwards up to 500 chars
    lookback = content[max(0, pos-500):pos]

    # Check if we're inside matches!()
    # Find last matches! and check if we're inside its parens
    matches_pos = lookback.rfind('matches!')
    if matches_pos >= 0:
        # Check if we're inside the matches! call
        actual_pos = max(0, pos-500) + matches_pos
        paren_pos = content.find('(', actual_pos)
        if paren_pos is not None and paren_pos < pos:
            close = find_matching_close_in_content(content, paren_pos)
            if close is not None and close > pos:
                return True

    # Check for match arm context: look for => after us, or | before/after us
    # Simple heuristic: find the previous line with => or {
    lines_before = lookback.split('\n')
    # If any recent line (within 5 lines) has a pattern-like structure
    for line in reversed(lines_before[-5:]):
        stripped = line.strip()
        if stripped.startswith('match ') or stripped.endswith('{'):
            return True
        if '=>' in stripped:
            # We're past a =>, so we're in value position
            return False

    # Look ahead for =>
    lookahead = content[pos:pos+200]
    first_arrow = lookahead.find('=>')
    first_semi = lookahead.find(';')
    first_eq = lookahead.find('= ')

    if first_arrow >= 0 and (first_semi < 0 or first_arrow < first_semi):
        # => comes before ;, likely pattern
        return True

    return False

def process_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    original = content
    changes = 0

    # Process tuple variants
    for variant, (old_args, new_args) in TUPLE_VARIANTS.items():
        pattern = f'Ty::{variant}('
        pos = 0
        iterations = 0
        while True:
            iterations += 1
            if iterations > 5000:
                break  # safety
            idx = content.find(pattern, pos)
            if idx < 0:
                break

            paren_start = idx + len(pattern) - 1  # position of (
            close = find_matching_close_in_content(content, paren_start)
            if close is None:
                pos = idx + 1
                continue

            # Count current args
            inner_start = paren_start + 1
            inner_end = close
            current_args = count_top_level_commas(content, inner_start, inner_end) + 1

            # Check if inner is empty (e.g., in macro context)
            inner = content[inner_start:inner_end].strip()
            if not inner:
                pos = close + 1
                continue

            if current_args == old_args:
                # Need to add TyAttr
                in_pattern = is_pattern_context(content, idx)
                if in_pattern:
                    insert = ', _'
                else:
                    insert = ', TyAttr::default()'
                content = content[:close] + insert + content[close:]
                changes += 1
                pos = close + len(insert) + 1
            elif current_args == new_args:
                # Already has the right number of args
                pos = close + 1
            elif current_args > new_args:
                # Too many args - might be a bug from previous script
                pos = close + 1
            else:
                pos = close + 1

    if changes > 0:
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"  {os.path.basename(filepath)}: {changes} tuple variant fixes")
    else:
        print(f"  {os.path.basename(filepath)}: no changes needed")

    return changes

FILES = ['builder.rs', 'lower_type_expr.rs', 'generics.rs', 'narrowing.rs',
         'throw_inference.rs', 'package_interface.rs', 'normalize.rs', 'inference.rs']

total = 0
for fname in FILES:
    fpath = os.path.join(BASE, fname)
    if os.path.exists(fpath):
        total += process_file(fpath)

print(f"\nTotal changes: {total}")
