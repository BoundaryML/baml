/// A sentinel value returned by streaming handles when the stream is done.
/// The reason this is used instead of `null` is that `null` is a valid return value,
/// whereas this class will probably never be used as a return value.
class StreamFinished {}

/// (internal)
///
/// A sentinel used when a partial parse didn't return anything and we should wait for more data.
class StreamNoYield {}
