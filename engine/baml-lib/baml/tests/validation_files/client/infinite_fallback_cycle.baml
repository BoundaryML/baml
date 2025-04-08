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
