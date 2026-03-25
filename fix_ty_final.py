#!/usr/bin/env python3
"""Final fix pass: targeted replacements for known remaining issues."""

import re
import os

BASE = "/Users/sam/baml2/baml_language/crates/baml_compiler2_tir/src"

def fix_file(filepath, replacements):
    """Apply line-targeted replacements to a file.
    replacements: list of (old_text, new_text) for global replacements
    """
    with open(filepath, 'r') as f:
        content = f.read()

    for old, new in replacements:
        content = content.replace(old, new)

    with open(filepath, 'w') as f:
        f.write(content)

def fix_value_underscores(filepath):
    """Replace _, _) in construction (value) positions with TyAttr::default()."""
    with open(filepath, 'r') as f:
        content = f.read()

    original = content

    # Fix Ty::Variant(..., _) in value positions
    # Pattern: Ty::Variant(args, _) where _ should be TyAttr::default()
    # We can detect value position by looking for:
    # - `= Ty::`, `return Ty::`, `=> Ty::`, `(Ty::`, `, Ty::`, `Some(Ty::`, `Box::new(Ty::`
    # - NOT after `|`, NOT in match arm left side

    # Simple approach: replace ALL `Ty::Variant(args, _)` where _ is the last element
    # and then fix the pattern ones back

    # Actually, let me just fix specific known bad patterns from the error output:
    # Lines with `, _)` or `, _);` that are in value context

    lines = content.split('\n')
    for i, line in enumerate(lines):
        stripped = line.strip()

        # Skip pattern context lines (match arm left side, matches!)
        if '=>' in line:
            # Split on => and only fix the right side
            parts = line.split('=>', 1)
            if len(parts) == 2:
                right = parts[1]
                new_right = right.replace(', _)', ', TyAttr::default())')
                new_right = new_right.replace(', _, _)', ', TyAttr::default(), TyAttr::default())')
                if new_right != right:
                    lines[i] = parts[0] + '=>' + new_right
            continue

        # If line is in an obvious value context (let, return, assignment, insert, etc.)
        if any(kw in stripped for kw in ['let ', 'return ', '.insert(', 'Some(', '= Ty::', 'Box::new(']):
            lines[i] = line.replace(', _)', ', TyAttr::default())')
            lines[i] = lines[i].replace(', _);', ', TyAttr::default());')
            lines[i] = lines[i].replace(', _,', ', TyAttr::default(),')
            continue

        # If line starts with Ty:: (continuation of value construction)
        if stripped.startswith('Ty::') or stripped.startswith('), _)'):
            lines[i] = line.replace(', _)', ', TyAttr::default())')
            continue

        # Standalone `, _)` at end of multi-line construction
        if stripped == ', _)' or stripped == '), _)' or stripped.endswith(', _)') or stripped.endswith('), _)'):
            # Check if we're in a value context by looking at surrounding lines
            # Look backwards for the start of this expression
            is_value = False
            for j in range(i-1, max(i-10, -1), -1):
                prev = lines[j].strip()
                if 'let ' in prev or 'return ' in prev or '= ' in prev or '.insert(' in prev:
                    is_value = True
                    break
                if '=>' in prev:
                    # Check if this is on the right side of =>
                    is_value = True
                    break
                if prev.startswith('match '):
                    is_value = False
                    break
            if is_value:
                lines[i] = line.replace(', _)', ', TyAttr::default())')

    content = '\n'.join(lines)

    if content != original:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False

def fix_pattern_defaults(filepath):
    """Replace TyAttr::default() in pattern positions with _."""
    with open(filepath, 'r') as f:
        content = f.read()

    original = content

    lines = content.split('\n')
    for i, line in enumerate(lines):
        # In match arm left side (before =>), replace TyAttr::default() with _
        if '=>' in line:
            parts = line.split('=>', 1)
            left = parts[0]
            if 'TyAttr::default()' in left:
                left = left.replace('TyAttr::default()', '_')
                lines[i] = left + '=>' + parts[1]
        # In matches!() macro
        elif 'matches!' in line and 'TyAttr::default()' in line:
            lines[i] = line.replace('TyAttr::default()', '_')

    content = '\n'.join(lines)

    if content != original:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False

# Process all files
FILES = ['builder.rs', 'lower_type_expr.rs', 'generics.rs', 'narrowing.rs',
         'throw_inference.rs', 'package_interface.rs', 'normalize.rs', 'inference.rs']

for fname in FILES:
    fpath = os.path.join(BASE, fname)
    if os.path.exists(fpath):
        changed1 = fix_value_underscores(fpath)
        changed2 = fix_pattern_defaults(fpath)
        if changed1 or changed2:
            print(f"  Fixed {fname}")
        else:
            print(f"  No changes: {fname}")

print("\nDone!")
