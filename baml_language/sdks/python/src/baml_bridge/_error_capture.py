"""Exception fields for the shared bounded host capture adapter."""

import types


def _attributes(error):
    kind = type(error)
    stored = BaseException.__dict__["__dict__"].__get__(error, kind)
    attributes = {}
    for key, value in dict.items(stored):
        if len(attributes) >= 512:
            break
        attributes[key] = value
    # Slots and builtin exception fields (e.g. OSError.filename) are stored
    # descriptors, unlike application properties which must never run here.
    scanned = 0
    for base in type.__dict__["__mro__"].__get__(kind)[:32]:
        for key, descriptor in type.__dict__["__dict__"].__get__(base).items():
            scanned += 1
            if len(attributes) >= 512 or scanned > 512:
                return attributes
            if (
                key in attributes
                or key in BaseException.__dict__
                or key.startswith("__")
            ):
                continue
            if type(descriptor) in (
                types.MemberDescriptorType,
                types.GetSetDescriptorType,
            ):
                try:
                    attributes[key] = descriptor.__get__(error, kind)
                except BaseException:
                    pass
    return attributes


def _exception_capture(error):
    kind = type(error)
    fields = {
        "type": type.__dict__["__qualname__"].__get__(kind),
        "module": type.__dict__["__dict__"].__get__(kind).get("__module__"),
    }
    try:
        fields["message"] = str(error)
    except BaseException:
        # A broken display hook must not discard the other exception fields.
        fields["message"] = {"$opaque": "exception_message"}
    fields["args"] = BaseException.args.__get__(error, kind)
    # Use builtin descriptors, bypassing subclass properties and attribute hooks.
    for name, descriptor in (
        ("cause", BaseException.__cause__),
        ("context", BaseException.__context__),
        ("suppress_context", BaseException.__suppress_context__),
    ):
        fields[name] = descriptor.__get__(error, kind)
    fields["attributes"] = _attributes(error)
    frames = []
    tb = BaseException.__traceback__.__get__(error, kind)
    while tb is not None and len(frames) < 64:
        code = tb.tb_frame.f_code
        frames.append(
            {"file": code.co_filename, "line": tb.tb_lineno, "function": code.co_name}
        )
        tb = tb.tb_next
    if tb is not None:
        frames.append({"$truncated": "frames"})
    fields["traceback"] = frames
    # ExceptionGroup is only available on Python 3.11+.
    try:
        group = BaseExceptionGroup
    except NameError:
        group = None
    if group is not None and any(
        base is group for base in type.__dict__["__mro__"].__get__(kind)
    ):
        fields["exceptions"] = group.exceptions.__get__(error, kind)
    return fields
