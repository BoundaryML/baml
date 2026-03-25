#!/usr/bin/env python3
"""Bulk-update Ty construction and match sites to add TyAttr.

Strategy:
- Parse cargo check JSON output to get error locations
- Classify each as pattern (needs { .. } or , _) vs value (needs TyAttr::default())
- Apply fixes by line number
"""

import json
import subprocess
import re
import sys
from collections import defaultdict

BASE = "/Users/sam/baml2/baml_language/crates/baml_compiler2_tir/src"

# Unit variants that became struct variants
UNIT_VARIANTS = ["Never", "Void", "BuiltinUnknown", "RustType", "Type", "Unknown", "Error"]

def get_errors():
    """Run cargo check and get structured errors."""
    result = subprocess.run(
        ["cargo", "check", "-p", "baml_compiler2_tir", "--message-format=json"],
        capture_output=True, text=True,
        cwd="/Users/sam/baml2/baml_language"
    )
    errors = []
    for line in result.stderr.split('\n') + result.stdout.split('\n'):
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
            if msg.get('reason') == 'compiler-message':
                errors.append(msg)
        except json.JSONDecodeError:
            pass
    return errors

def classify_and_fix():
    """Parse errors and apply fixes."""
    errors = get_errors()

    # Group fixes by file
    fixes = defaultdict(list)  # file -> [(line, col, error_code, message, suggestion)]

    for err in errors:
        message = err.get('message', {})
        code = message.get('code', {})
        if isinstance(code, dict):
            code = code.get('code', '')
        else:
            code = str(code) if code else ''

        msg_text = message.get('message', '')
        spans = message.get('spans', [])
        children = message.get('children', [])

        if not spans:
            continue

        primary = None
        for s in spans:
            if s.get('is_primary'):
                primary = s
                break
        if not primary:
            primary = spans[0]

        fname = primary.get('file_name', '')
        line_start = primary.get('line_start', 0)
        col_start = primary.get('column_start', 0)

        # Get suggestion from children
        suggestion = ''
        for child in children:
            if child.get('message', '').startswith('provide the argument') or \
               child.get('message', '').startswith('use `_`') or \
               child.get('message', '').startswith('you might have meant') or \
               child.get('message', '').startswith("the struct variant's field"):
                suggestion = child.get('message', '')
                break

        fixes[fname].append({
            'line': line_start,
            'col': col_start,
            'code': code,
            'message': msg_text,
            'suggestion': suggestion,
            'full_msg': message,
        })

    return fixes

def apply_fixes_to_file(filepath, file_fixes):
    """Apply all fixes to a single file."""
    with open(filepath, 'r') as f:
        lines = f.readlines()

    # Sort fixes by line number in reverse so line numbers stay valid
    file_fixes.sort(key=lambda x: (x['line'], x['col']), reverse=True)

    # Deduplicate by line (take first = highest priority since reversed)
    seen_lines = {}
    unique_fixes = []
    for fix in file_fixes:
        key = (fix['line'], fix['col'])
        if key not in seen_lines:
            seen_lines[key] = True
            unique_fixes.append(fix)

    for fix in unique_fixes:
        line_idx = fix['line'] - 1
        if line_idx < 0 or line_idx >= len(lines):
            continue

        line = lines[line_idx]
        msg = fix['message']
        code = fix['code']

        if 'expected value, found struct variant' in msg:
            # Value position: Ty::Variant -> Ty::Variant { attr: TyAttr::default() }
            for v in UNIT_VARIANTS:
                pattern = f'Ty::{v}'
                if pattern in line:
                    # Don't replace if already has braces
                    # Find the pattern and check what follows
                    idx = line.find(pattern)
                    while idx >= 0:
                        end = idx + len(pattern)
                        rest = line[end:].lstrip()
                        if not rest.startswith('{') and not rest.startswith('('):
                            line = line[:end] + ' { attr: TyAttr::default() }' + line[end:]
                            break
                        idx = line.find(pattern, end)
            lines[line_idx] = line

        elif 'expected unit struct, unit variant or constant' in msg:
            # Pattern position: Ty::Variant -> Ty::Variant { .. }
            for v in UNIT_VARIANTS:
                pattern = f'Ty::{v}'
                if pattern in line:
                    idx = line.find(pattern)
                    while idx >= 0:
                        end = idx + len(pattern)
                        rest = line[end:].lstrip()
                        if not rest.startswith('{') and not rest.startswith('('):
                            line = line[:end] + ' { .. }' + line[end:]
                            break
                        idx = line.find(pattern, end)
            lines[line_idx] = line

        elif 'this pattern has' in msg and 'field' in msg and 'tuple variant has' in msg:
            # Pattern with wrong number of fields - add trailing _, or ,_
            # e.g., Ty::List(elem_ty) needs Ty::List(elem_ty, _)
            # e.g., Ty::Map(_, val_ty) needs Ty::Map(_, val_ty, _)
            # Find the pattern on this line and add ,_ before the closing paren
            # We need to find which variant and add the right number of _

            # Extract how many fields are expected vs found
            m_fields = re.search(r'this pattern has (\d+) field.*, but the corresponding tuple variant has (\d+) field', msg)
            if m_fields:
                found = int(m_fields.group(1))
                expected = int(m_fields.group(2))
                missing = expected - found

                # Find the Ty:: pattern on this line at the right column
                col = fix['col'] - 1
                # Find the closing paren for this pattern
                # Look for the pattern starting near col
                # Simple approach: find Ty::Variant( and add _, before )
                for v in ['Class', 'Enum', 'EnumVariant', 'TypeAlias', 'Primitive',
                          'List', 'Map', 'Union', 'Optional', 'Literal',
                          'EvolvingList', 'EvolvingMap', 'TypeVar']:
                    pat = f'Ty::{v}('
                    idx = line.find(pat, max(0, col - 30))
                    if idx >= 0 and idx <= col + len(f'Ty::{v}'):
                        # Find matching closing paren
                        start = idx + len(pat)
                        depth = 1
                        pos = start
                        while pos < len(line) and depth > 0:
                            if line[pos] == '(':
                                depth += 1
                            elif line[pos] == ')':
                                depth -= 1
                            pos += 1
                        if depth == 0:
                            close_pos = pos - 1
                            insert = ', _' * missing
                            line = line[:close_pos] + insert + line[close_pos:]
                            lines[line_idx] = line
                        break

        elif code == 'E0061' and 'argument' in msg and 'TyAttr' in msg:
            # Construction with wrong number of args - add TyAttr::default()
            # e.g., Ty::Primitive(PrimitiveType::Null) -> add , TyAttr::default()
            # Find the closing paren and insert before it
            col = fix['col'] - 1
            for v in ['Class', 'Enum', 'EnumVariant', 'TypeAlias', 'Primitive',
                      'List', 'Map', 'Union', 'Optional', 'Literal',
                      'EvolvingList', 'EvolvingMap', 'TypeVar']:
                pat = f'Ty::{v}('
                idx = line.find(pat, max(0, col - 30))
                if idx >= 0 and idx <= col + len(f'Ty::{v}') + 2:
                    # Find matching closing paren
                    start = idx + len(pat)
                    depth = 1
                    pos = start
                    while pos < len(line) and depth > 0:
                        if line[pos] == '(':
                            depth += 1
                        elif line[pos] == ')':
                            depth -= 1
                        pos += 1
                    if depth == 0:
                        close_pos = pos - 1
                        line = line[:close_pos] + ', TyAttr::default()' + line[close_pos:]
                        lines[line_idx] = line
                    break

        elif code == 'E0063' and 'missing' in msg and 'attr' in msg:
            # Struct variant missing attr field
            # e.g., Ty::Function { params, ret } -> add , attr: TyAttr::default()
            # Find closing brace and insert before it
            col = fix['col'] - 1
            close_brace = line.rfind('}')
            if close_brace >= 0:
                line = line[:close_brace] + ', attr: TyAttr::default() ' + line[close_brace:]
                lines[line_idx] = line

        elif code == 'E0026' and 'does not have a field named' in msg:
            pass  # Skip these for now
        elif code == 'E0027' and 'pattern does not mention field' in msg and 'attr' in msg:
            # Pattern missing the attr field
            # Find closing brace and add , attr: _ or .. before it
            col = fix['col'] - 1
            # Check if there's a } on this line we can add .. before
            close_brace = line.rfind('}')
            if close_brace >= 0:
                # Check if there's already a ..
                before_brace = line[:close_brace].rstrip()
                if '..' not in before_brace:
                    line = line[:close_brace] + ', .. ' + line[close_brace:]
                    lines[line_idx] = line

    with open(filepath, 'w') as f:
        f.writelines(lines)

def add_imports():
    """Add TyAttr imports to all files that need them."""
    import glob
    for fpath in glob.glob(f"{BASE}/*.rs"):
        if fpath.endswith('/ty.rs'):
            continue
        with open(fpath, 'r') as f:
            content = f.read()

        if 'TyAttr' in content:
            continue
        if 'Ty::' not in content:
            continue

        # Add TyAttr to existing crate::ty import
        new_content = re.sub(
            r'(use crate::\s*\{[^}]*?ty::\{)([^}]+?)(\})',
            lambda m: m.group(1) + m.group(2).rstrip() + ', TyAttr' + m.group(3),
            content,
            count=1
        )
        if new_content != content:
            with open(fpath, 'w') as f:
                f.write(new_content)
            print(f"  Added TyAttr import to {os.path.basename(fpath)} (nested)")
            continue

        # Try ty::{...} pattern
        new_content = re.sub(
            r'(use crate::ty::\{)([^}]+?)(\};)',
            lambda m: m.group(1) + m.group(2).rstrip() + ', TyAttr' + m.group(3),
            content,
            count=1
        )
        if new_content != content:
            with open(fpath, 'w') as f:
                f.write(new_content)
            print(f"  Added TyAttr import to {os.path.basename(fpath)} (direct)")
            continue

        print(f"  Could not add TyAttr import to {os.path.basename(fpath)}")

import os

print("Step 1: Adding TyAttr imports...")
add_imports()

print("\nStep 2: Getting compiler errors...")
fixes = classify_and_fix()

print(f"\nStep 3: Applying fixes to {len(fixes)} files...")
for fname, file_fixes in fixes.items():
    full_path = os.path.join("/Users/sam/baml2/baml_language", fname)
    if os.path.exists(full_path):
        print(f"  {fname}: {len(file_fixes)} fixes")
        apply_fixes_to_file(full_path, file_fixes)
    else:
        print(f"  SKIP {fname} (not found)")

print("\nDone!")
