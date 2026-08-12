/// Universal wrapper for errors that do not implement a known capability channel.
class UnknownError {
    data: unknown,
    message: string[],
}
