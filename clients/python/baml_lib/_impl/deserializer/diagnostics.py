import typing


class DeserializerException(Exception):
    def __init__(
        self,
        errors: typing.List["DeserializerError"],
        warnings: typing.List["DeserializerWarning"],
        raw_string: str,
    ) -> None:
        super().__init__(errors)
        self.__items = errors + warnings
        self.__items.sort(key=lambda x: x.scope, reverse=True)
        self.__num_errors = len(errors)
        self.__num_warnings = len(warnings)
        self.__raw_string = raw_string

    def __str__(self) -> str:
        output = [
            f"Failed to Deserialize: ({self.__num_errors} errors) ({self.__num_warnings} warnings)"
        ]
        for i in self.__items:
            output.append("------")
            output.append(str(i))
        output.append("------")
        output.append("Raw:")
        output.append(self.__raw_string)
        return "\n".join(output)


class Diagnostics:
    __scope_errors: typing.Dict[str, typing.List["DeserializerError"]]
    __errors: typing.List["DeserializerError"]
    __warnings: typing.List["DeserializerWarning"]
    __scope: typing.List[str]

    def __init__(self, raw_string: str) -> None:
        self.__errors = []
        self.__warnings = []
        self.__raw_string = raw_string
        self.__scope = []
        self.__scope_errors = {}

    def push_scope(self, scope: str) -> None:
        self.__scope.append(scope)
        self.__scope_errors[".".join(self.__scope)] = []

    def pop_scope(self, errors_as_warnings: bool) -> None:
        prev_scope_key = ".".join(self.__scope)
        self.__scope.pop()
        # If there are any errors, convert them to warnings.
        for error in self.__scope_errors.get(prev_scope_key, []):
            if errors_as_warnings:
                self.__push_warning(error.to_warning())
            else:
                self.__push_error(error)

    def __push_error(self, error: "DeserializerError") -> None:
        if len(self.__scope) > 0:
            key = ".".join(self.__scope)
            if key in self.__scope_errors:
                self.__scope_errors[key].append(error)
        else:
            self.__errors.append(error)

    def __push_warning(self, warning: "DeserializerWarning") -> None:
        self.__warnings.append(warning)

    def to_exception(self) -> None:
        """
        This method raises a DeserializerException if there are any errors in the diagnostics.
        """
        if len(self.__errors) > 0:
            raise DeserializerException(
                self.__errors, self.__warnings, self.__raw_string
            )

    def push_unkown_warning(self, message: str) -> None:
        self.__push_warning(DeserializerWarning(self.__scope, message))

    def push_missing_keys_error(self, keys: typing.List[str]) -> None:
        self.__push_error(
            DeserializerError(self.__scope, f"Missing keys: {', '.join(keys)}")
        )

    def push_enum_error(
        self, enum_name: str, value: typing.Any, expected: typing.List[str]
    ) -> None:
        self.__push_error(
            DeserializerError(
                self.__scope,
                f'Failed to parse `{value}` as `{enum_name}`. Expected one of: {", ".join(expected)}',
            )
        )

    def push_unknown_error(self, message: str) -> None:
        self.__push_error(DeserializerError(self.__scope, message))


class DeserializerError:
    def __init__(self, scope: typing.List[str], message: str):
        self.__message = message
        self.__scope = list(scope)

    @property
    def scope(self) -> typing.List[str]:
        return self.__scope

    def __str__(self) -> str:
        if len(self.__scope) == 0:
            return f"Error: {self.__message}"
        return f"Error in {'.'.join(self.__scope)}: {self.__message}"

    def __repr__(self) -> str:
        if len(self.__scope) == 0:
            return f"Error: {self.__message}"
        return f"Error in {'.'.join(self.__scope)}: {self.__message}"

    def to_warning(self) -> "DeserializerWarning":
        return DeserializerWarning(self.__scope, self.__message)


class DeserializerWarning:
    def __init__(self, scope: typing.List[str], message: str):
        self.__message = message
        self.__scope = list(scope)

    @property
    def scope(self) -> typing.List[str]:
        return self.__scope

    def __str__(self) -> str:
        if len(self.__scope) == 0:
            return f"Warning: {self.__message}"
        return f"Warning in {'.'.join(self.__scope)}: {self.__message}"

    def __repr__(self) -> str:
        if len(self.__scope) == 0:
            return f"Warning: {self.__message}"
        return f"Warning in {'.'.join(self.__scope)}: {self.__message}"
