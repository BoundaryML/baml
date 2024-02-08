// TypeScript equivalent of Python's 'abc' and 'typing' modules
abstract class RawWrapper {
    // Abstract method as_str
    abstract as_str(inner: boolean): string | undefined;

    // Abstract method as_smart_str
    abstract as_smart_str(inner: boolean): string | undefined;

    // Abstract method as_self
    abstract as_self(): any;

    // Abstract method as_int
    abstract as_int(): number | undefined;

    // Abstract method as_float
    abstract as_float(): number | undefined;

    // Abstract method as_bool
    abstract as_bool(): boolean | undefined;

    // Abstract method as_list
    abstract as_list(): Iterable<RawWrapper>;

    // Abstract method as_dict
    abstract as_dict(): Iterable<[RawWrapper | undefined, RawWrapper]>;
}

export { RawWrapper };
