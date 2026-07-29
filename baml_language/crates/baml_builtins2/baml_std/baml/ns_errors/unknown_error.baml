/// Universal wrapper for errors that do not implement a known capability channel.
class UnknownError {
  data: unknown,
  message: string[],

  /// RECOVERY — call once, at the handler. Reassert the channel: a bare or
  /// already-wrapped T is restored to its identity (the wrapper's breadcrumbs
  /// end their job here); an existing wrapper around anything else passes
  /// through untouched; any other value is wrapped fresh. Never nests.
  function from<T>(data: unknown) -> T | UnknownError throws never {
    match (data) {
      let value: T => value,
      UnknownError { data: let inner: T } => inner,
      let e: UnknownError => e,
      _ => UnknownError { data: data, message: [] },
    }
  }

  /// PROPAGATION — call while an error bubbles through a layer. Never unwraps:
  /// a known T rides the typed channel untouched; an existing wrapper gains one
  /// breadcrumb; anything else is wrapped fresh with the message. Layers with
  /// no typed channel of their own pass `T = never`.
  function with_message<T>(data: unknown, message: string) -> T | UnknownError throws never {
    match (data) {
      let e: UnknownError => {
        e.message = e.message.concat([message]);
        e
      },
      let value: T => value,
      _ => UnknownError { data: data, message: [message] },
    }
  }
}
