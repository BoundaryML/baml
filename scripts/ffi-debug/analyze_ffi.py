#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "matplotlib",
# ]
# ///
"""
Analyze FFI lifecycle logs to find memory leaks and visualize object lifetimes.

When running main program set env vars:
USE ENV VAR: BAML_FFI_LOG=ffi.log
USE ENV VAR: BAML_FFI_CLIENT_LOG=client.log

Then after running main program, run this script:
```sh
./analyze_ffi.py ffi.log client.log
./analyze_ffi.py ffi.log client.log --plot  # Generate timeline plot
```

This will output a summary of the FFI lifecycle and any leaks found.

Log format (with timestamps):
  [FFI_CREATE ts=1234567890] type=XXX ptr=0xYYY       - New object created via from_object()
  [FFI_WRAP_ARC ts=1234567890] type=XXX ptr=0xYYY    - Existing Arc wrapped via from_arc()
  [FFI_GIVE_GO ts=1234567890] type=XXX ptr=0xYYY     - Pointer given to Go via pointer()
  [FFI_GO_RELEASE ts=1234567890] type=XXX ptr=0xYYY  - Go called destructor, releasing reference
  [FFI_FREE ts=1234567890] type=XXX ptr=0xYYY        - Object freed (last reference dropped)
  [FFI_BUF_ALLOC ts=1234567890] ptr=0xYYY len=NNN    - Buffer allocated for Go
  [FFI_BUF_FREE ts=1234567890] ptr=0xYYY len=NNN     - Buffer freed by Go
  [FFI_ERROR_ALLOC ts=1234567890] ptr=0xYYY len=NNN  - Error string allocated
  [FFI_ERROR_FREE ts=1234567890] ptr=0xYYY           - Error string freed
  [FFI_CSTRING_ALLOC ts=1234567890] ptr=0xYYY len=NNN - CString allocated (version, fallback)
  [FFI_CSTRING_FREE ts=1234567890] ptr=0xYYY         - CString freed

For leak detection:
  - Every FFI_GIVE_GO should have a matching FFI_GO_RELEASE (Go released its ref)
  - Every FFI_BUF_ALLOC should have a matching FFI_BUF_FREE (Go freed the buffer)
  - Every FFI_ERROR_ALLOC should have a matching FFI_ERROR_FREE
  - Every FFI_CSTRING_ALLOC should have a matching FFI_CSTRING_FREE
  - FFI_FREE only fires when the Arc's last reference drops (may be held by runtime)
"""

import sys
import re
import argparse
from collections import defaultdict

def parse_log(filename):
    # Match FFI_* (core) and CLIENT_{LANG}_* (client SDK) events with optional timestamp
    # Format: [EVENT_NAME ts=123456] type=XXX ptr=0xYYY
    # or legacy: [EVENT_NAME] type=XXX ptr=0xYYY
    pattern = re.compile(r'\[(FFI|CLIENT_GO|CLIENT_RUST)_(\w+)(?:\s+ts=(\d+))?\] type=([^ \[\]]+) ptr=(0x[0-9a-f]+)')
    # Also match callback events: [CLIENT_GO_CALLBACK_ADD ts=123] id=X map_size=Y
    callback_pattern = re.compile(r'\[CLIENT_GO_CALLBACK_(ADD|DEL)(?:\s+ts=(\d+))?\] id=(\d+) map_size=(\d+)')
    # Match destructor errors from any client
    error_pattern = re.compile(r'\[CLIENT_(?:GO|RUST)_DESTRUCTOR_ERROR(?:\s+ts=(\d+))?\] type=([^ ]+) ptr=(0x[0-9a-f]+) error=(.*)')
    # Match buffer allocation/free events: [FFI_BUF_ALLOC ts=123] ptr=0xYYY len=NNN
    buffer_pattern = re.compile(r'\[FFI_BUF_(ALLOC|FREE)(?:\s+ts=(\d+))?\] ptr=(0x[0-9a-f]+) len=(\d+)')
    # Match error string allocation/free: [FFI_ERROR_ALLOC ts=123] ptr=0xYYY len=NNN
    error_str_pattern = re.compile(r'\[FFI_ERROR_(ALLOC|FREE)(?:\s+ts=(\d+))?\] ptr=(0x[0-9a-f]+)(?:\s+len=(\d+))?')
    # Match cstring allocation/free: [FFI_CSTRING_ALLOC ts=123] ptr=0xYYY len=NNN
    cstring_pattern = re.compile(r'\[FFI_CSTRING_(ALLOC|FREE)(?:\s+ts=(\d+))?\] ptr=(0x[0-9a-f]+)(?:\s+len=(\d+))?')

    events = []
    callback_events = []
    error_events = []
    buffer_events = []
    error_str_events = []
    cstring_events = []
    interleaved_lines = 0
    with open(filename) as f:
        for line_num, line in enumerate(f, 1):
            # Find ALL event matches on the line (handles interleaved log output)
            matches = pattern.findall(line)
            if len(matches) > 1:
                interleaved_lines += 1
            for match in matches:
                prefix = match[0]  # FFI or CLIENT_GO or CLIENT_RUST
                event_type = match[1]
                timestamp = int(match[2]) if match[2] else None
                type_name = match[3]
                ptr = match[4]
                events.append((line_num, prefix, event_type, type_name, ptr, timestamp))

            # Check for callback events
            cb_matches = callback_pattern.findall(line)
            for cb_match in cb_matches:
                cb_type = cb_match[0]  # ADD or DEL
                timestamp = int(cb_match[1]) if cb_match[1] else None
                cb_id = int(cb_match[2])
                map_size = int(cb_match[3])
                callback_events.append((line_num, cb_type, cb_id, map_size, timestamp))

            # Check for error events
            err_matches = error_pattern.findall(line)
            for err_match in err_matches:
                timestamp = int(err_match[0]) if err_match[0] else None
                type_name = err_match[1]
                ptr = err_match[2]
                error_msg = err_match[3]
                error_events.append((line_num, type_name, ptr, error_msg, timestamp))

            # Check for buffer events
            buf_matches = buffer_pattern.findall(line)
            for buf_match in buf_matches:
                buf_type = buf_match[0]  # ALLOC or FREE
                timestamp = int(buf_match[1]) if buf_match[1] else None
                ptr = buf_match[2]
                length = int(buf_match[3])
                buffer_events.append((line_num, buf_type, ptr, length, timestamp))

            # Check for error string events
            err_str_matches = error_str_pattern.findall(line)
            for err_str_match in err_str_matches:
                err_type = err_str_match[0]  # ALLOC or FREE
                timestamp = int(err_str_match[1]) if err_str_match[1] else None
                ptr = err_str_match[2]
                length = int(err_str_match[3]) if err_str_match[3] else 0
                error_str_events.append((line_num, err_type, ptr, length, timestamp))

            # Check for cstring events
            cstr_matches = cstring_pattern.findall(line)
            for cstr_match in cstr_matches:
                cstr_type = cstr_match[0]  # ALLOC or FREE
                timestamp = int(cstr_match[1]) if cstr_match[1] else None
                ptr = cstr_match[2]
                length = int(cstr_match[3]) if cstr_match[3] else 0
                cstring_events.append((line_num, cstr_type, ptr, length, timestamp))

    if interleaved_lines > 0:
        print(f"Note: Found {interleaved_lines} lines with interleaved log output (concurrent Rust/Go logging)")

    return events, callback_events, error_events, buffer_events, error_str_events, cstring_events

def analyze(events, callback_events=None, error_events=None, buffer_events=None, error_str_events=None, cstring_events=None):
    # Track all pointers - Rust runtime side (FFI_*)
    given_to_go = defaultdict(list)  # ptr -> [(line_num, type_name, ts), ...]
    go_released = defaultdict(list)  # ptr -> [(line_num, type_name, ts), ...]
    freed = defaultdict(list)        # ptr -> [(line_num, type_name, ts), ...]
    created = defaultdict(list)      # ptr -> [(line_num, type_name, ts), ...]
    wrapped = defaultdict(list)      # ptr -> [(line_num, type_name, ts), ...]

    # Track Go side events
    go_received = defaultdict(list)   # ptr -> [(line_num, type_name, ts), ...]
    go_destructor_start = defaultdict(list) # ptr -> [(line_num, type_name, ts), ...]
    go_destructor_ok = defaultdict(list)    # ptr -> [(line_num, type_name, ts), ...]

    # Track Rust SDK side events (CLIENT_RUST_*)
    sdk_received = defaultdict(list)         # ptr -> [(line_num, type_name, ts), ...]
    sdk_created = defaultdict(list)          # ptr -> [(line_num, type_name, ts), ...]
    sdk_destructor_start = defaultdict(list) # ptr -> [(line_num, type_name, ts), ...]
    sdk_destructor_ok = defaultdict(list)    # ptr -> [(line_num, type_name, ts), ...]
    sdk_destructor_error = defaultdict(list) # ptr -> [(line_num, type_name, ts), ...]

    for line_num, prefix, event_type, type_name, ptr, ts in events:
        if prefix == 'FFI':
            if event_type == 'CREATE':
                created[ptr].append((line_num, type_name, ts))
            elif event_type == 'WRAP_ARC':
                wrapped[ptr].append((line_num, type_name, ts))
            elif event_type == 'GIVE_GO':
                given_to_go[ptr].append((line_num, type_name, ts))
            elif event_type == 'GO_RELEASE':
                go_released[ptr].append((line_num, type_name, ts))
            elif event_type == 'FREE':
                freed[ptr].append((line_num, type_name, ts))
        elif prefix == 'CLIENT_GO':
            if event_type == 'RECEIVE':
                go_received[ptr].append((line_num, type_name, ts))
            elif event_type == 'DESTRUCTOR_START':
                go_destructor_start[ptr].append((line_num, type_name, ts))
            elif event_type == 'DESTRUCTOR_OK':
                go_destructor_ok[ptr].append((line_num, type_name, ts))
        elif prefix == 'CLIENT_RUST':
            if event_type == 'RECEIVE':
                sdk_received[ptr].append((line_num, type_name, ts))
            elif event_type == 'CREATE':
                sdk_created[ptr].append((line_num, type_name, ts))
            elif event_type == 'DESTRUCTOR_START':
                sdk_destructor_start[ptr].append((line_num, type_name, ts))
            elif event_type == 'DESTRUCTOR_OK':
                sdk_destructor_ok[ptr].append((line_num, type_name, ts))
            elif event_type == 'DESTRUCTOR_ERROR':
                sdk_destructor_error[ptr].append((line_num, type_name, ts))

    total_give = sum(len(v) for v in given_to_go.values())
    total_release = sum(len(v) for v in go_released.values())
    total_go_receive = sum(len(v) for v in go_received.values())
    total_go_destructor_start = sum(len(v) for v in go_destructor_start.values())
    total_go_destructor_ok = sum(len(v) for v in go_destructor_ok.values())
    total_go_destructor_error = len(error_events) if error_events else 0

    total_sdk_receive = sum(len(v) for v in sdk_received.values())
    total_sdk_create = sum(len(v) for v in sdk_created.values())
    total_sdk_destructor_start = sum(len(v) for v in sdk_destructor_start.values())
    total_sdk_destructor_ok = sum(len(v) for v in sdk_destructor_ok.values())
    total_sdk_destructor_error = sum(len(v) for v in sdk_destructor_error.values())

    # Process buffer events
    buffer_allocs = defaultdict(list)  # ptr -> [(line_num, length, ts), ...]
    buffer_frees = defaultdict(list)   # ptr -> [(line_num, length, ts), ...]
    if buffer_events:
        for line_num, buf_type, ptr, length, ts in buffer_events:
            if buf_type == 'ALLOC':
                buffer_allocs[ptr].append((line_num, length, ts))
            elif buf_type == 'FREE':
                buffer_frees[ptr].append((line_num, length, ts))

    total_buf_alloc = sum(len(v) for v in buffer_allocs.values())
    total_buf_free = sum(len(v) for v in buffer_frees.values())
    total_buf_bytes_alloc = sum(l for events in buffer_allocs.values() for _, l, _ in events)
    total_buf_bytes_free = sum(l for events in buffer_frees.values() for _, l, _ in events)

    # Process error string events
    error_str_allocs = defaultdict(list)  # ptr -> [(line_num, length, ts), ...]
    error_str_frees = defaultdict(list)   # ptr -> [(line_num, length, ts), ...]
    if error_str_events:
        for line_num, err_type, ptr, length, ts in error_str_events:
            if err_type == 'ALLOC':
                error_str_allocs[ptr].append((line_num, length, ts))
            elif err_type == 'FREE':
                error_str_frees[ptr].append((line_num, length, ts))

    total_err_str_alloc = sum(len(v) for v in error_str_allocs.values())
    total_err_str_free = sum(len(v) for v in error_str_frees.values())

    # Process cstring events
    cstring_allocs = defaultdict(list)  # ptr -> [(line_num, length, ts), ...]
    cstring_frees = defaultdict(list)   # ptr -> [(line_num, length, ts), ...]
    if cstring_events:
        for line_num, cstr_type, ptr, length, ts in cstring_events:
            if cstr_type == 'ALLOC':
                cstring_allocs[ptr].append((line_num, length, ts))
            elif cstr_type == 'FREE':
                cstring_frees[ptr].append((line_num, length, ts))

    total_cstr_alloc = sum(len(v) for v in cstring_allocs.values())
    total_cstr_free = sum(len(v) for v in cstring_frees.values())

    print(f"=== FFI Lifecycle Analysis ===\n")
    print(f"Rust runtime side (FFI_*):")
    print(f"  FFI_CREATE:     {sum(len(v) for v in created.values()):>6}")
    print(f"  FFI_WRAP_ARC:   {sum(len(v) for v in wrapped.values()):>6}")
    print(f"  FFI_GIVE_GO:    {total_give:>6}  <- Rust gives ptr to Go/SDK")
    print(f"  FFI_GO_RELEASE: {total_release:>6}  <- Rust destructor called")
    print(f"  FFI_FREE:       {sum(len(v) for v in freed.values()):>6}  <- Arc fully freed")
    print()

    # Only print buffer section if there are buffer events
    if total_buf_alloc > 0:
        print(f"Buffer allocations (FFI_BUF_*):")
        print(f"  FFI_BUF_ALLOC:  {total_buf_alloc:>6}  ({total_buf_bytes_alloc:,} bytes)")
        print(f"  FFI_BUF_FREE:   {total_buf_free:>6}  ({total_buf_bytes_free:,} bytes)")
        if total_buf_alloc != total_buf_free:
            print(f"  LEAK:           {total_buf_alloc - total_buf_free:>6}  ({total_buf_bytes_alloc - total_buf_bytes_free:,} bytes)")
        print()

    # Only print error string section if there are events
    if total_err_str_alloc > 0:
        print(f"Error strings (FFI_ERROR_*):")
        print(f"  FFI_ERROR_ALLOC:{total_err_str_alloc:>6}")
        print(f"  FFI_ERROR_FREE: {total_err_str_free:>6}")
        if total_err_str_alloc != total_err_str_free:
            print(f"  LEAK:           {total_err_str_alloc - total_err_str_free:>6}  <- Error strings not freed!")
        print()

    # Only print cstring section if there are events
    if total_cstr_alloc > 0:
        print(f"CStrings (FFI_CSTRING_*):")
        print(f"  FFI_CSTRING_ALLOC:{total_cstr_alloc:>6}  (version, fallback strings)")
        print(f"  FFI_CSTRING_FREE: {total_cstr_free:>6}")
        if total_cstr_alloc != total_cstr_free:
            print(f"  LEAK:             {total_cstr_alloc - total_cstr_free:>6}  <- CStrings not freed!")
        print()

    print(f"Go client (CLIENT_GO_*):")
    print(f"  CLIENT_GO_RECEIVE:         {total_go_receive:>6}  <- Go receives object")
    print(f"  CLIENT_GO_DESTRUCTOR_START:{total_go_destructor_start:>6}  <- Go finalizer starts")
    print(f"  CLIENT_GO_DESTRUCTOR_OK:   {total_go_destructor_ok:>6}  <- Go finalizer succeeds")
    print(f"  CLIENT_GO_DESTRUCTOR_ERROR:{total_go_destructor_error:>6}  <- Go finalizer fails")
    print()

    # Only print Rust client section if there are client events
    if total_sdk_receive > 0 or total_sdk_create > 0:
        print(f"Rust client (CLIENT_RUST_*):")
        print(f"  CLIENT_RUST_RECEIVE:         {total_sdk_receive:>6}  <- Rust client receives object")
        print(f"  CLIENT_RUST_CREATE:          {total_sdk_create:>6}  <- Rust client creates object")
        print(f"  CLIENT_RUST_DESTRUCTOR_START:{total_sdk_destructor_start:>6}  <- Rust client destructor starts")
        print(f"  CLIENT_RUST_DESTRUCTOR_OK:   {total_sdk_destructor_ok:>6}  <- Rust client destructor succeeds")
        print(f"  CLIENT_RUST_DESTRUCTOR_ERROR:{total_sdk_destructor_error:>6}  <- Rust client destructor fails")
        print()

    # Find Rust-side leaks: pointers given to Go but never released
    rust_leaks = []
    for ptr in given_to_go:
        give_count = len(given_to_go[ptr])
        release_count = len(go_released.get(ptr, []))
        if give_count > release_count:
            leak_count = give_count - release_count
            types = set(t for _, t, _ in given_to_go[ptr])
            rust_leaks.append((ptr, types, leak_count))

    if rust_leaks:
        print(f"=== RUST-SIDE LEAKS ({len(rust_leaks)} unique pointers) ===")
        print(f"(FFI_GIVE_GO without matching FFI_GO_RELEASE)\n")
        by_type = defaultdict(list)
        for ptr, types, count in rust_leaks:
            for t in types:
                by_type[t].append((ptr, count))

        for type_name in sorted(by_type.keys()):
            ptrs = by_type[type_name]
            total_leaked = sum(c for _, c in ptrs)
            print(f"{type_name}:")
            print(f"    {total_leaked} leaked references ({len(ptrs)} unique pointers)")
        print()
    else:
        print("=== NO RUST-SIDE LEAKS ===")
        print("All FFI_GIVE_GO have matching FFI_GO_RELEASE.\n")

    # Find Go-side leaks: pointers received by Go but never destructed (or destructor failed)
    go_leaks = []
    for ptr in go_received:
        receive_count = len(go_received[ptr])
        # Count successful destructor calls only
        destructor_ok_count = len(go_destructor_ok.get(ptr, []))
        if receive_count > destructor_ok_count:
            leak_count = receive_count - destructor_ok_count
            types = set(t for _, t, _ in go_received[ptr])
            go_leaks.append((ptr, types, leak_count))

    if go_leaks:
        print(f"=== GO CLIENT LEAKS ({len(go_leaks)} unique pointers) ===")
        print(f"(CLIENT_GO_RECEIVE without matching CLIENT_GO_DESTRUCTOR_OK)\n")
        by_type = defaultdict(list)
        for ptr, types, count in go_leaks:
            for t in types:
                by_type[t].append((ptr, count))

        for type_name in sorted(by_type.keys()):
            ptrs = by_type[type_name]
            total_leaked = sum(c for _, c in ptrs)
            print(f"{type_name}:")
            print(f"    {total_leaked} leaked references ({len(ptrs)} unique pointers)")
        print()
    else:
        print("=== NO GO CLIENT LEAKS ===")
        print("All CLIENT_GO_RECEIVE have matching CLIENT_GO_DESTRUCTOR_OK.\n")

    # Find Rust SDK-side leaks: pointers received by SDK but never destructed
    sdk_leaks = []
    for ptr in sdk_received:
        receive_count = len(sdk_received[ptr])
        destructor_ok_count = len(sdk_destructor_ok.get(ptr, []))
        if receive_count > destructor_ok_count:
            leak_count = receive_count - destructor_ok_count
            types = set(t for _, t, _ in sdk_received[ptr])
            sdk_leaks.append((ptr, types, leak_count))

    # Also check created objects
    for ptr in sdk_created:
        if ptr not in sdk_received:  # Don't double count
            create_count = len(sdk_created[ptr])
            destructor_ok_count = len(sdk_destructor_ok.get(ptr, []))
            if create_count > destructor_ok_count:
                leak_count = create_count - destructor_ok_count
                types = set(t for _, t, _ in sdk_created[ptr])
                sdk_leaks.append((ptr, types, leak_count))

    if sdk_leaks:
        print(f"=== RUST CLIENT LEAKS ({len(sdk_leaks)} unique pointers) ===")
        print(f"(CLIENT_RUST_RECEIVE/CREATE without matching CLIENT_RUST_DESTRUCTOR_OK)\n")
        by_type = defaultdict(list)
        for ptr, types, count in sdk_leaks:
            for t in types:
                by_type[t].append((ptr, count))

        for type_name in sorted(by_type.keys()):
            ptrs = by_type[type_name]
            total_leaked = sum(c for _, c in ptrs)
            print(f"{type_name}:")
            print(f"    {total_leaked} leaked references ({len(ptrs)} unique pointers)")
        print()
    elif total_sdk_receive > 0 or total_sdk_create > 0:
        print("=== NO RUST CLIENT LEAKS ===")
        print("All CLIENT_RUST_RECEIVE/CREATE have matching CLIENT_RUST_DESTRUCTOR_OK.\n")

    # Compare Rust vs Go leaks
    rust_leak_ptrs = set(ptr for ptr, _, _ in rust_leaks)
    go_leak_ptrs = set(ptr for ptr, _, _ in go_leaks)

    only_rust = rust_leak_ptrs - go_leak_ptrs
    only_go = go_leak_ptrs - rust_leak_ptrs
    both = rust_leak_ptrs & go_leak_ptrs

    print(f"=== LEAK COMPARISON ===")
    print(f"Leaked on Rust side only: {len(only_rust)} pointers")
    print(f"Leaked on Go side only:   {len(only_go)} pointers")
    print(f"Leaked on both sides:     {len(both)} pointers")

    if only_rust:
        print(f"\nRust-only leaks (FFI_GIVE_GO but no FFI_GO_RELEASE, yet Go finalized):")
        by_type = defaultdict(list)
        for ptr in only_rust:
            for _, t, _ in given_to_go[ptr]:
                by_type[t].append(ptr)
        for t in sorted(by_type.keys()):
            print(f"  {t}: {len(by_type[t])} pointers")

    if only_go:
        print(f"\nGo-only leaks (GO_RECEIVE but no GO_DESTRUCTOR, yet Rust released):")
        by_type = defaultdict(list)
        for ptr in only_go:
            for _, t, _ in go_received[ptr]:
                by_type[t].append(ptr)
        for t in sorted(by_type.keys()):
            print(f"  {t}: {len(by_type[t])} pointers")
    print()

    # Callback analysis
    if callback_events:
        adds = [e for e in callback_events if e[1] == 'ADD']
        dels = [e for e in callback_events if e[1] == 'DEL']
        final_size = callback_events[-1][3] if callback_events else 0
        print(f"Callback map:")
        print(f"  Total ADDs: {len(adds)}")
        print(f"  Total DELs: {len(dels)}")
        print(f"  Final map size: {final_size}")
        if len(adds) != len(dels):
            print(f"  WARNING: {len(adds) - len(dels)} callbacks not cleaned up!")
        print()

    # Error events breakdown
    if error_events:
        print(f"\n=== DESTRUCTOR ERRORS ({len(error_events)} total) ===")
        by_type = defaultdict(list)
        by_error = defaultdict(int)
        for line_num, type_name, ptr, error_msg, ts in error_events:
            by_type[type_name].append((ptr, error_msg))
            by_error[error_msg] += 1

        print(f"\nBy type:")
        for type_name in sorted(by_type.keys()):
            print(f"  {type_name}: {len(by_type[type_name])} errors")

        print(f"\nBy error message (top 10):")
        for error_msg, count in sorted(by_error.items(), key=lambda x: -x[1])[:10]:
            truncated = error_msg[:80] + "..." if len(error_msg) > 80 else error_msg
            print(f"  {count:>5}x {truncated}")
        print()

    # Summary
    print("=== SUMMARY ===")
    if total_go_destructor_error > 0:
        print(f"GO CLIENT ERRORS: {total_go_destructor_error} finalizers failed!")

    if total_go_receive != total_go_destructor_ok:
        print(f"GO CLIENT LEAK: {total_go_receive - total_go_destructor_ok} objects received but not successfully finalized")
        print(f"  (Received: {total_go_receive}, Success: {total_go_destructor_ok}, Error: {total_go_destructor_error})")
    elif total_go_receive > 0:
        print(f"GO CLIENT OK: All {total_go_receive} objects were properly finalized")

    if total_give != total_release:
        print(f"FFI CORE LEAK: {total_give - total_release} pointers given but destructor not called")
        print(f"  (Given: {total_give}, Released: {total_release})")
    elif total_give > 0:
        print(f"FFI CORE OK: All {total_give} pointers properly released")

    # Rust client summary
    total_sdk_objects = total_sdk_receive + total_sdk_create
    if total_sdk_destructor_error > 0:
        print(f"RUST CLIENT ERRORS: {total_sdk_destructor_error} destructors failed!")

    if total_sdk_objects > 0:
        if total_sdk_objects != total_sdk_destructor_ok:
            print(f"RUST CLIENT LEAK: {total_sdk_objects - total_sdk_destructor_ok} objects not properly cleaned up")
            print(f"  (Received/Created: {total_sdk_objects}, Destructed OK: {total_sdk_destructor_ok}, Error: {total_sdk_destructor_error})")
        else:
            print(f"RUST CLIENT OK: All {total_sdk_objects} objects were properly destructed")

    # Buffer summary
    buffer_leak_count = 0
    if total_buf_alloc > 0:
        if total_buf_alloc != total_buf_free:
            buffer_leak_count = total_buf_alloc - total_buf_free
            print(f"BUFFER LEAK: {buffer_leak_count} buffers allocated but not freed")
            print(f"  (Allocated: {total_buf_alloc}, Freed: {total_buf_free}, Leaked bytes: {total_buf_bytes_alloc - total_buf_bytes_free:,})")
        else:
            print(f"BUFFERS OK: All {total_buf_alloc} buffers properly freed")

    # Error string summary
    error_str_leak_count = 0
    if total_err_str_alloc > 0:
        if total_err_str_alloc != total_err_str_free:
            error_str_leak_count = total_err_str_alloc - total_err_str_free
            print(f"ERROR STRING LEAK: {error_str_leak_count} error strings not freed")
            print(f"  (Allocated: {total_err_str_alloc}, Freed: {total_err_str_free})")
        else:
            print(f"ERROR STRINGS OK: All {total_err_str_alloc} error strings properly freed")

    # CString summary
    cstring_leak_count = 0
    if total_cstr_alloc > 0:
        if total_cstr_alloc != total_cstr_free:
            cstring_leak_count = total_cstr_alloc - total_cstr_free
            print(f"CSTRING LEAK: {cstring_leak_count} CStrings not freed (version/fallback)")
            print(f"  (Allocated: {total_cstr_alloc}, Freed: {total_cstr_free})")
        else:
            print(f"CSTRINGS OK: All {total_cstr_alloc} CStrings properly freed")

    return len(rust_leaks) + len(go_leaks) + len(sdk_leaks) + buffer_leak_count + error_str_leak_count + cstring_leak_count


def generate_plot(events, callback_events, buffer_events, output_file):
    """Generate a timeline plot showing object counts over time."""
    try:
        import matplotlib.pyplot as plt
        import matplotlib.dates as mdates
        from datetime import datetime
    except ImportError:
        print("Error: matplotlib is required for plotting.")
        print("Run with: uv run --script analyze_ffi.py (auto-installs deps)")
        print("Or install manually: pip install matplotlib")
        sys.exit(1)

    # Filter events with timestamps - include ptr for tracking specific items
    timed_events = [(ts, prefix, event_type, type_name, ptr) for _, prefix, event_type, type_name, ptr, ts in events if ts]

    # Check for events without timestamps
    events_without_ts = [(prefix, event_type, type_name, ptr) for _, prefix, event_type, type_name, ptr, ts in events if not ts]
    if events_without_ts:
        print(f"\nWARNING: {len(events_without_ts)} events without timestamps (excluded from tracking):")
        by_type = defaultdict(int)
        for prefix, event_type, type_name, ptr in events_without_ts:
            by_type[f"{prefix}_{event_type}"] += 1
        for event_key, count in sorted(by_type.items()):
            print(f"  {event_key}: {count}")

    # Add buffer events to timed events (with special prefix)
    if buffer_events:
        for _, buf_type, ptr, length, ts in buffer_events:
            if ts:
                timed_events.append((ts, 'FFI_BUF', buf_type, f'len={length}', ptr))

    if not timed_events:
        print("No timestamped events found. Cannot generate plot.")
        return

    # Sort by timestamp
    timed_events.sort(key=lambda x: x[0])

    # Track object counts over time
    # For FFI core: track objects "in flight" to Go (GIVE_GO - GO_RELEASE)
    # For Go client: track objects held by Go (RECEIVE - DESTRUCTOR_OK)
    # For Rust client: track objects held by Rust SDK (RECEIVE/CREATE - DESTRUCTOR_OK)
    # For buffers: track buffers allocated but not freed (ALLOC - FREE)

    timestamps = []
    ffi_in_flight = []
    go_held = []
    rust_held = []
    buffers_held = []

    current_ffi = 0
    current_go = 0
    current_rust = 0
    current_buf = 0

    # Track which specific items are currently held (for leak details)
    # Use lists to handle multiple references to same pointer (Arc wrapping)
    ffi_items = defaultdict(list)      # ptr -> [(type_name, ts), ...]
    go_items = defaultdict(list)       # ptr -> [(type_name, ts), ...]
    rust_items = defaultdict(list)     # ptr -> [(type_name, ts), ...]
    buffer_items = defaultdict(list)   # ptr -> [(type_name/len, ts), ...]

    for ts, prefix, event_type, type_name, ptr in timed_events:
        if prefix == 'FFI':
            if event_type == 'GIVE_GO':
                current_ffi += 1
                ffi_items[ptr].append((type_name, ts))
            elif event_type == 'GO_RELEASE':
                current_ffi -= 1
                if ffi_items[ptr]:
                    ffi_items[ptr].pop()
                if not ffi_items[ptr]:
                    del ffi_items[ptr]
        elif prefix == 'CLIENT_GO':
            if event_type == 'RECEIVE':
                current_go += 1
                go_items[ptr].append((type_name, ts))
            elif event_type == 'DESTRUCTOR_OK':
                current_go -= 1
                if go_items[ptr]:
                    go_items[ptr].pop()
                if not go_items[ptr]:
                    del go_items[ptr]
        elif prefix == 'CLIENT_RUST':
            if event_type in ('RECEIVE', 'CREATE'):
                current_rust += 1
                rust_items[ptr].append((type_name, ts))
            elif event_type == 'DESTRUCTOR_OK':
                current_rust -= 1
                if rust_items[ptr]:
                    rust_items[ptr].pop()
                if not rust_items[ptr]:
                    del rust_items[ptr]
        elif prefix == 'FFI_BUF':
            if event_type == 'ALLOC':
                current_buf += 1
                buffer_items[ptr].append((type_name, ts))
            elif event_type == 'FREE':
                current_buf -= 1
                if buffer_items[ptr]:
                    buffer_items[ptr].pop()
                if not buffer_items[ptr]:
                    del buffer_items[ptr]

        # Convert microseconds to datetime
        dt = datetime.fromtimestamp(ts / 1_000_000)
        timestamps.append(dt)
        ffi_in_flight.append(current_ffi)
        go_held.append(current_go)
        rust_held.append(current_rust)
        buffers_held.append(current_buf)

    # Downsample if too many points (matplotlib can't handle millions of points)
    # Always preserve the final point for accurate final counts
    MAX_POINTS = 10000
    if len(timestamps) > MAX_POINTS:
        print(f"Downsampling {len(timestamps):,} points to {MAX_POINTS:,} for plotting...")
        step = len(timestamps) // MAX_POINTS
        # Take every Nth point, but always include the last point
        sampled_indices = list(range(0, len(timestamps), step))
        if sampled_indices[-1] != len(timestamps) - 1:
            sampled_indices.append(len(timestamps) - 1)
        timestamps = [timestamps[i] for i in sampled_indices]
        ffi_in_flight = [ffi_in_flight[i] for i in sampled_indices]
        go_held = [go_held[i] for i in sampled_indices]
        rust_held = [rust_held[i] for i in sampled_indices]
        buffers_held = [buffers_held[i] for i in sampled_indices]

    # Determine number of subplots (skip empty panels)
    has_buffers = any(b != 0 for b in buffers_held)
    has_rust = any(r != 0 for r in rust_held)
    n_plots = 2 + (1 if has_rust else 0) + (1 if has_buffers else 0)

    # Create the plot
    fig, axes = plt.subplots(n_plots, 1, figsize=(14, 3 * n_plots + 1), sharex=True)
    if n_plots == 1:
        axes = [axes]
    fig.suptitle('FFI Object Lifecycle Over Time', fontsize=14)

    # Plot FFI in-flight objects
    ax_idx = 0
    axes[ax_idx].fill_between(timestamps, ffi_in_flight, alpha=0.3, color='blue')
    axes[ax_idx].plot(timestamps, ffi_in_flight, color='blue', linewidth=0.5)
    axes[ax_idx].set_ylabel('FFI In-Flight\n(GIVE_GO - GO_RELEASE)')
    axes[ax_idx].set_ylim(bottom=0)
    axes[ax_idx].grid(True, alpha=0.3)
    final_ffi = ffi_in_flight[-1] if ffi_in_flight else 0
    axes[ax_idx].axhline(y=0, color='green', linestyle='--', alpha=0.5, label='Expected: 0')
    if final_ffi != 0:
        axes[ax_idx].axhline(y=final_ffi, color='red', linestyle='--', alpha=0.5, label=f'Final: {final_ffi} (LEAK!)')
    axes[ax_idx].legend(loc='upper right')

    # Plot Go-held objects
    ax_idx += 1
    axes[ax_idx].fill_between(timestamps, go_held, alpha=0.3, color='green')
    axes[ax_idx].plot(timestamps, go_held, color='green', linewidth=0.5)
    axes[ax_idx].set_ylabel('Go Client Held\n(RECEIVE - DESTRUCTOR_OK)')
    axes[ax_idx].set_ylim(bottom=0)
    axes[ax_idx].grid(True, alpha=0.3)
    final_go = go_held[-1] if go_held else 0
    axes[ax_idx].axhline(y=0, color='green', linestyle='--', alpha=0.5, label='Expected: 0')
    if final_go != 0:
        axes[ax_idx].axhline(y=final_go, color='red', linestyle='--', alpha=0.5, label=f'Final: {final_go} (LEAK!)')
    axes[ax_idx].legend(loc='upper right')

    # Plot Rust SDK-held objects (if any)
    if has_rust:
        ax_idx += 1
        axes[ax_idx].fill_between(timestamps, rust_held, alpha=0.3, color='orange')
        axes[ax_idx].plot(timestamps, rust_held, color='orange', linewidth=0.5)
        axes[ax_idx].set_ylabel('Rust Client Held\n(RECEIVE/CREATE - DESTRUCTOR_OK)')
        axes[ax_idx].set_ylim(bottom=0)
        axes[ax_idx].grid(True, alpha=0.3)
        final_rust = rust_held[-1] if rust_held else 0
        axes[ax_idx].axhline(y=0, color='green', linestyle='--', alpha=0.5, label='Expected: 0')
        if final_rust != 0:
            axes[ax_idx].axhline(y=final_rust, color='red', linestyle='--', alpha=0.5, label=f'Final: {final_rust} (LEAK!)')
        axes[ax_idx].legend(loc='upper right')

    # Plot buffers held (if any)
    if has_buffers:
        ax_idx += 1
        axes[ax_idx].fill_between(timestamps, buffers_held, alpha=0.3, color='purple')
        axes[ax_idx].plot(timestamps, buffers_held, color='purple', linewidth=0.5)
        axes[ax_idx].set_ylabel('Buffers Held\n(ALLOC - FREE)')
        axes[ax_idx].set_ylim(bottom=0)
        axes[ax_idx].grid(True, alpha=0.3)
        final_buf = buffers_held[-1] if buffers_held else 0
        axes[ax_idx].axhline(y=0, color='green', linestyle='--', alpha=0.5, label='Expected: 0')
        if final_buf != 0:
            axes[ax_idx].axhline(y=final_buf, color='red', linestyle='--', alpha=0.5, label=f'Final: {final_buf} (LEAK!)')
        axes[ax_idx].legend(loc='upper right')

    # Set xlabel on last axis
    axes[-1].set_xlabel('Time')

    # Format x-axis
    for ax in axes:
        ax.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))

    plt.tight_layout()
    plt.savefig(output_file, dpi=150, bbox_inches='tight')
    print(f"\nPlot saved to: {output_file}")

    # Also show peak values
    final_rust = rust_held[-1] if rust_held else 0
    final_buf = buffers_held[-1] if buffers_held else 0
    print(f"\nPeak object counts:")
    print(f"  FFI in-flight:    {max(ffi_in_flight) if ffi_in_flight else 0}")
    print(f"  Go client held:   {max(go_held) if go_held else 0}")
    if has_rust:
        print(f"  Rust client held: {max(rust_held) if rust_held else 0}")
    if has_buffers:
        print(f"  Buffers held:     {max(buffers_held) if buffers_held else 0}")
    print(f"\nFinal object counts (should be 0):")
    print(f"  FFI in-flight:    {final_ffi}{'  <- LEAK!' if final_ffi != 0 else ''}")
    print(f"  Go client held:   {final_go}{'  <- LEAK!' if final_go != 0 else ''}")
    if has_rust:
        print(f"  Rust client held: {final_rust}{'  <- LEAK!' if final_rust != 0 else ''}")
    if has_buffers:
        print(f"  Buffers held:     {final_buf}{'  <- LEAK!' if final_buf != 0 else ''}")

    # Show details of leaked items if count is small (< 50)
    DETAIL_THRESHOLD = 50

    def format_timestamp(ts):
        """Format timestamp as relative time from start"""
        if timestamps:
            start_ts = timed_events[0][0]
            delta_ms = (ts - start_ts) / 1000
            return f"+{delta_ms:.1f}ms"
        return str(ts)

    def print_leaked_items(name, items_dict):
        """Print details of leaked items grouped by type"""
        if not items_dict:
            return
        # Count total items (sum of all list lengths)
        total_count = sum(len(refs) for refs in items_dict.values())
        unique_ptrs = len(items_dict)
        if total_count > DETAIL_THRESHOLD:
            print(f"\n  {name}: {total_count} items in {unique_ptrs} pointers (too many to list, showing types only)")
            by_type = defaultdict(int)
            for ptr, refs in items_dict.items():
                for type_name, ts in refs:
                    by_type[type_name] += 1
            for type_name in sorted(by_type.keys()):
                print(f"    {type_name}: {by_type[type_name]}")
        else:
            print(f"\n  {name}: {total_count} items in {unique_ptrs} pointers still held at end:")
            # Group by type
            by_type = defaultdict(list)
            for ptr, refs in items_dict.items():
                for type_name, ts in refs:
                    by_type[type_name].append((ptr, ts))
            for type_name in sorted(by_type.keys()):
                items = by_type[type_name]
                print(f"    {type_name} ({len(items)}):")
                for ptr, ts in sorted(items, key=lambda x: x[1]):
                    print(f"      {ptr} (created {format_timestamp(ts)})")

    # Show details of any leaked items
    has_leaks = final_ffi != 0 or final_go != 0 or final_rust != 0 or final_buf != 0

    if has_leaks:
        if ffi_items:
            print_leaked_items("FFI in-flight", ffi_items)
        if go_items:
            print_leaked_items("Go client held", go_items)
        if rust_items:
            print_leaked_items("Rust client held", rust_items)
        if buffer_items:
            print_leaked_items("Buffers held", buffer_items)


def main():
    parser = argparse.ArgumentParser(description='Analyze FFI lifecycle logs for memory leaks')
    parser.add_argument('files', nargs='+', help='Log files to analyze')
    parser.add_argument('--plot', '-p', action='store_true', help='Generate timeline plot')
    parser.add_argument('--output', '-o', default='ffi_timeline.png', help='Output file for plot (default: ffi_timeline.png)')
    args = parser.parse_args()

    all_events = []
    all_callback_events = []
    all_error_events = []
    all_buffer_events = []
    all_error_str_events = []
    all_cstring_events = []

    for filename in args.files:
        events, callback_events, error_events, buffer_events, error_str_events, cstring_events = parse_log(filename)
        all_events.extend(events)
        all_callback_events.extend(callback_events)
        all_error_events.extend(error_events)
        all_buffer_events.extend(buffer_events)
        all_error_str_events.extend(error_str_events)
        all_cstring_events.extend(cstring_events)
        print(f"Loaded {len(events)} events, {len(buffer_events)} buffer, {len(error_str_events)} error_str, {len(cstring_events)} cstring from {filename}")

    if not all_events and not all_buffer_events and not all_error_str_events and not all_cstring_events:
        print(f"No FFI events found in provided files")
        sys.exit(1)

    print(f"\nTotal: {len(all_events)} FFI events, {len(all_callback_events)} callback, {len(all_error_events)} destructor_error, {len(all_buffer_events)} buffer, {len(all_error_str_events)} error_str, {len(all_cstring_events)} cstring\n")
    leak_count = analyze(all_events, all_callback_events, all_error_events, all_buffer_events, all_error_str_events, all_cstring_events)

    if args.plot:
        generate_plot(all_events, all_callback_events, all_buffer_events, args.output)

    sys.exit(1 if leak_count > 0 else 0)

if __name__ == '__main__':
    main()
