enum TransportKind {
  Http
  HttpSse
  WebSocket
  WebRtc
}

enum StateOwnership {
  ClientOwned
  ServerStoredById
  ServerSession
}

interface TransportInfo requires Provider {
  function transport(self) -> TransportKind throws never
  function state_ownership(self) -> StateOwnership throws never
}
