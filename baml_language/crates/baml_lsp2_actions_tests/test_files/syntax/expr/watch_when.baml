function Top() -> int {
  watch let x = 10;
  x.$watch.options(baml.WatchOptions{ when: GoodFunction });
  x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
  x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
  1
}

function GoodFunction(i: int) -> bool {
  true
}

function BadFunction1(i: int, j: int) -> bool {
  true
}

function BadFunction2(i: string) -> bool {
  true
}

//----
//- diagnostics
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:3:3 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │   ────────┬───────
//    │           ╰───────── type `int` has no member `$watch`
//    │
//    │ Note: Error code: E0007
// ───╯
// error: unresolved type: baml.WatchOptions
//    ╭─[ expr_watch_when.baml:3:20 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │                    ───────────────────┬───────────────────
//    │                                       ╰───────────────────── unresolved type: baml.WatchOptions
//    │
//    │ Note: Error code: E0002
// ───╯
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:4:3 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │   ────────┬───────
//    │           ╰───────── type `int` has no member `$watch`
//    │
//    │ Note: Error code: E0007
// ───╯
// error: unresolved type: baml.WatchOptions
//    ╭─[ expr_watch_when.baml:4:20 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │                    ───────────────────┬───────────────────
//    │                                       ╰───────────────────── unresolved type: baml.WatchOptions
//    │
//    │ Note: Error code: E0002
// ───╯
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:5:3 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │   ────────┬───────
//    │           ╰───────── type `int` has no member `$watch`
//    │
//    │ Note: Error code: E0007
// ───╯
// error: unresolved type: baml.WatchOptions
//    ╭─[ expr_watch_when.baml:5:20 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │                    ───────────────────┬───────────────────
//    │                                       ╰───────────────────── unresolved type: baml.WatchOptions
//    │
//    │ Note: Error code: E0002
// ───╯
