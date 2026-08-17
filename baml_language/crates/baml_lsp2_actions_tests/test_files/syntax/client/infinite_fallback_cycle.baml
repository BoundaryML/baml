client<llm> SelfReferentialClient {
  provider fallback
  options {
    strategy [SelfReferentialClient]
  }
}

client<llm> ClientA {
  provider fallback
  options {
    strategy [ClientB]
  }
}

client<llm> ClientB {
  provider fallback
  options {
    strategy [ClientC]
  }
}

client<llm> ClientC {
  provider fallback
  options {
    strategy [ClientA]
  }
}

// error: Error validating: These fallback clients form a dependency cycle: SelfReferentialClient
//   -->  client/infinite_fallback_cycle.baml:1
//    |
//    |
//  1 | client<llm> SelfReferentialClient {
//  2 |   provider fallback
//  3 |   options {
//  4 |     strategy [SelfReferentialClient]
//  5 |   }
//  6 | }
//    |
// error: Error validating: These fallback clients form a dependency cycle: ClientA -> ClientB -> ClientC
//   -->  client/infinite_fallback_cycle.baml:8
//    |
//  7 |
//  8 | client<llm> ClientA {
//  9 |   provider fallback
// 10 |   options {
// 11 |     strategy [ClientB]
// 12 |   }
// 13 | }
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client SelfReferentialClient = ai.Fallback.new(members = [FirstClient, SecondClient]);` (compose reliability with
//   │ ai.Retry / ai.Fallback / ai.RoundRobin)
//    ╭─[client_infinite_fallback_cycle.baml:1:1]
//  1 │ ╭─▶ client<llm> SelfReferentialClient {
//  2 │ │     provider fallback
//  3 │ │     options {
//  4 │ │       strategy [SelfReferentialClient]
//  5 │ │     }
//  6 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ClientA = ai.Fallback.new(members = [FirstClient, SecondClient]);` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_infinite_fallback_cycle.baml:8:1]
//   8 │ ╭─▶ client<llm> ClientA {
//   9 │ │     provider fallback
//  10 │ │     options {
//  11 │ │       strategy [ClientB]
//  12 │ │     }
//  13 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ClientB = ai.Fallback.new(members = [FirstClient, SecondClient]);` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_infinite_fallback_cycle.baml:15:1]
//  15 │ ╭─▶ client<llm> ClientB {
//  16 │ │     provider fallback
//  17 │ │     options {
//  18 │ │       strategy [ClientC]
//  19 │ │     }
//  20 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ClientC = ai.Fallback.new(members = [FirstClient, SecondClient]);` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_infinite_fallback_cycle.baml:22:1]
//  22 │ ╭─▶ client<llm> ClientC {
//  23 │ │     provider fallback
//  24 │ │     options {
//  25 │ │       strategy [ClientA]
//  26 │ │     }
//  27 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
