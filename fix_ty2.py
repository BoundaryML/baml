#!/usr/bin/env python3
"""Fix remaining Ty construction/pattern sites using cargo check JSON output.

Handles multi-line constructions by tracking paren depth across lines.
"""

import json
import subprocess
import re
import sys
import os
from collections import defaultdict

def get_errors():
    result = subprocess.run(
        ["cargo", "check", "-p", "baml_compiler2_tir", "--message-format=json"],
        capture_output=True, text=True,
        cwd="/Users/sam/baml2/baml_language"
    )
    errors = []
    for line in (result.stderr + '\n' + result.stdout).split('\n'):
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
            if msg.get('reason') == 'compiler-message':
                errors.append(msg['message'])
        except json.JSONDecodeError:
            pass
    return errors

def find_matching_close(lines, start_line, start_col, open_char='(', close_char=')'):
    """Find matching closing paren/brace across lines."""
    depth = 0
    in_string = False
    line_idx = start_line
    col = start_col

    while line_idx < len(lines):
        line = lines[line_idx]
        while col < len(line):
            ch = line[col]
            if ch == '"' and (col == 0 or line[col-1] != '\\'):
                in_string = not in_string
            elif not in_string:
                if ch == open_char:
                    depth += 1
                elif ch == close_char:
                    depth -= 1
                    if depth == 0:
                        return (line_idx, col)
            col += 1
        line_idx += 1
        col = 0
    return None

def apply_fixes():
    errors = get_errors()

    # Group by file
    file_fixes = defaultdict(list)

    for msg in errors:
        code = msg.get('code', {})
        if isinstance(code, dict):
            code = code.get('code', '')
        else:
            code = str(code) if code else ''

        msg_text = msg.get('message', '')
        spans = msg.get('spans', [])

        primary = None
        for s in spans:
            if s.get('is_primary'):
                primary = s
                break
        if not primary and spans:
            primary = spans[0]
        if not primary:
            continue

        fname = primary['file_name']
        line = primary['line_start']
        col = primary['column_start']

        file_fixes[fname].append({
            'code': code,
            'message': msg_text,
            'line': line,
            'col': col,
            'line_end': primary.get('line_end', line),
            'col_end': primary.get('column_end', col),
        })

    for fname, fixes in file_fixes.items():
        full_path = os.path.join("/Users/sam/baml2/baml_language", fname)
        if not os.path.exists(full_path):
            continue

        with open(full_path, 'r') as f:
            lines = f.readlines()

        # Sort fixes by position in reverse order
        fixes.sort(key=lambda x: (x['line'], x['col']), reverse=True)

        # Deduplicate
        seen = set()
        unique = []
        for fix in fixes:
            key = (fix['line'], fix['col'], fix['code'])
            if key not in seen:
                seen.add(key)
                unique.append(fix)
        fixes = unique

        changes = 0
        for fix in fixes:
            code = fix['code']
            msg = fix['message']
            line_idx = fix['line'] - 1
            col_idx = fix['col'] - 1

            if code == 'E0061' and 'TyAttr' in msg:
                # Missing TyAttr argument in construction
                # Find the opening ( of the Ty:: variant
                line = lines[line_idx]
                # Find Ty:: on or before this line
                search_start = max(0, col_idx - 50)
                paren_pos = line.find('(', col_idx)

                # Look backwards from col for Ty::
                found_ty = False
                for v in ['Class', 'Enum', 'EnumVariant', 'TypeAlias', 'Primitive',
                         'List', 'Map', 'Union', 'Optional', 'Literal',
                         'EvolvingList', 'EvolvingMap', 'TypeVar']:
                    pat = f'Ty::{v}('
                    # Search this line and previous lines
                    for search_line in range(line_idx, max(line_idx - 3, -1), -1):
                        idx = lines[search_line].find(pat)
                        if idx >= 0:
                            paren_start = idx + len(pat) - 1  # position of (
                            result = find_matching_close(lines, search_line, paren_start)
                            if result:
                                close_line, close_col = result
                                # Insert , TyAttr::default() before the closing )
                                l = lines[close_line]
                                lines[close_line] = l[:close_col] + ', TyAttr::default()' + l[close_col:]
                                changes += 1
                                found_ty = True
                            break
                    if found_ty:
                        break

            elif code == 'E0063' and 'attr' in msg:
                # Missing attr field in Function struct construction
                # Find the closing } and insert , attr: TyAttr::default()
                line = lines[line_idx]
                # Find Ty::Function on or before this line
                for search_line in range(line_idx, max(line_idx - 5, -1), -1):
                    if 'Ty::Function' in lines[search_line]:
                        brace_idx = lines[search_line].find('{', lines[search_line].find('Ty::Function'))
                        if brace_idx >= 0:
                            result = find_matching_close(lines, search_line, brace_idx, '{', '}')
                            if result:
                                close_line, close_col = result
                                l = lines[close_line]
                                # Check if there's already attr: in the block
                                block_text = ''
                                for bl in range(search_line, close_line + 1):
                                    block_text += lines[bl]
                                if 'attr:' not in block_text:
                                    lines[close_line] = l[:close_col] + ', attr: TyAttr::default() ' + l[close_col:]
                                    changes += 1
                        break

            elif code == 'E0023' and 'pattern has' in msg:
                m = re.search(r'this pattern has (\d+) field.*, but the corresponding tuple variant has (\d+) field', msg)
                if m:
                    found = int(m.group(1))
                    expected = int(m.group(2))

                    if found > expected:
                        # Too many fields - the script added extra _, need to remove
                        # Skip for now, handle manually
                        continue

                    missing = expected - found
                    # Find the closing ) in the pattern
                    line = lines[line_idx]
                    for v in ['Class', 'Enum', 'EnumVariant', 'TypeAlias', 'Primitive',
                             'List', 'Map', 'Union', 'Optional', 'Literal',
                             'EvolvingList', 'EvolvingMap', 'TypeVar']:
                        pat = f'Ty::{v}('
                        for search_line in range(line_idx, max(line_idx - 3, -1), -1):
                            idx = lines[search_line].find(pat)
                            if idx >= 0:
                                paren_start = idx + len(pat) - 1
                                result = find_matching_close(lines, search_line, paren_start)
                                if result:
                                    close_line, close_col = result
                                    l = lines[close_line]
                                    insert = ', _' * missing
                                    lines[close_line] = l[:close_col] + insert + l[close_col:]
                                    changes += 1
                                break
                        else:
                            continue
                        break

            elif code == 'E0164' and 'TyAttr::default' in msg:
                # TyAttr::default() used in pattern - replace with _
                line = lines[line_idx]
                line = line.replace('TyAttr::default()', '_')
                lines[line_idx] = line
                changes += 1

            elif code == 'E0025' and 'attr' in msg and 'bound multiple times' in msg:
                # attr bound multiple times - fix the doubled attr
                line = lines[line_idx]
                # Remove one of the duplicate ", attr: TyAttr::default()" or similar
                line = re.sub(r',\s*attr:\s*TyAttr::default\(\)\s*,\s*\.\.\s*,\s*attr:\s*TyAttr::default\(\)', '', line)
                lines[line_idx] = line
                changes += 1

            elif code == 'E0797' and 'base expression required after `..`' in msg:
                # `..` used incorrectly - replace with just ignoring attr
                line = lines[line_idx]
                # This is likely ", .. " before } - need to fix
                # Actually this usually means `.. ` was used without a base struct
                # In pattern context, `..` is fine, in construction it's not
                # Let's look at context
                pass

            elif code == 'E0027' and 'pattern does not mention field `attr`' in msg:
                # Pattern needs to add `..` or `attr: _`
                line = lines[line_idx]
                # Find closing } on this or nearby lines
                for search_line in range(line_idx, min(line_idx + 5, len(lines))):
                    if '}' in lines[search_line]:
                        brace_pos = lines[search_line].rfind('}')
                        before = lines[search_line][:brace_pos].rstrip()
                        if '..' not in before and 'attr' not in lines[search_line][:brace_pos]:
                            lines[search_line] = lines[search_line][:brace_pos] + ', .. ' + lines[search_line][brace_pos:]
                            changes += 1
                        break

            elif code == 'E0026' and 'TypeExpr::Function' in msg:
                # Wrong variant - this is TypeExpr::Function not Ty::Function
                # The script incorrectly added attr to TypeExpr::Function
                line = lines[line_idx]
                # Remove the erroneous attr addition
                line = re.sub(r',\s*attr:\s*TyAttr::default\(\)\s*', '', line)
                lines[line_idx] = line
                changes += 1

        if changes > 0:
            with open(full_path, 'w') as f:
                f.writelines(lines)
            print(f"  {fname}: {changes} fixes applied")

if __name__ == '__main__':
    print("Applying fixes...")
    apply_fixes()
    print("Done!")
