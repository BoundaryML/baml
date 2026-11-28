"""How much another Python thread runs while the main thread makes long
BAML calls, with the GIL held or released during turns."""
import sys, threading, time
import baml_sdk, baml_native
CALLS = 50
from baml_sdk.bridge_test import make_ints_sync

def probe(release):
    baml_native.set_release_gil(release)
    stop = False
    count = 0
    def spin():
        nonlocal count
        while not stop:
            count += 1
    t = threading.Thread(target=spin); t.start()
    time.sleep(0.05)
    count = 0
    started = time.perf_counter()
    for _ in range(CALLS):
        make_ints_sync(1_000_000)
    elapsed = time.perf_counter() - started
    stop = True; t.join()
    return elapsed / CALLS, count

for release in (False, True, False, True, False, True):
    per, count = probe(release)
    print(f"release={release}: {per*1e3:.1f} ms per make_ints(1M) call; other thread: {count/1e6:.2f} M iterations")
