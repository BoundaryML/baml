from __future__ import annotations

import os
import typing
import uuid
import platform
import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))


class EnvVars:
    __var_list: typing.Dict[str, None | str]

    def __init__(self, var_list: typing.List[typing.Tuple[str, str] | str]):
        # List of environment variables you're interested in.
        # This can be predefined or passed during instantiation.
        self.__dict__["__var_list"] = {
            (var if isinstance(var, str) else var[0]): None
            if isinstance(var, str)
            else var[1]
            for var in var_list
        }

    @property
    def var_list(self) -> typing.Dict[str, None | str]:
        """Get the list of environment variables."""
        return typing.cast(
            typing.Dict[str, typing.Optional[str]], self.__dict__["__var_list"]
        )

    def __getattr__(self, key: str) -> str:
        """Get the value of an environment variable when accessed as an attribute."""
        default_val = self.var_list.get(key, None)
        val = os.environ.get(key, default_val)
        if val is None:
            raise ValueError(f"'{key}' must be set via CLI.")
        return val

    def __setattr__(self, key: str, value: str) -> None:
        """Set the value of an environment variable when accessed as an attribute."""
        assert isinstance(key, str), f"{key} must be a string."
        assert isinstance(value, str), f"{key}: {value} must be a string."

        if key in self.__var_list:
            os.environ[key] = value
        else:
            raise ValueError(f"'{key}' must be set via CLI.")

    def list_all(self) -> typing.Dict[str, None | str]:
        """List all environment variables specified in the var_list."""
        return {key: os.environ.get(key, None) for key in self.__dict__["__var_list"]}

    def __str__(self) -> str:
        """String representation of the environment variables."""
        return str(self.list_all())


ENV = EnvVars(
    var_list=[
        "GLOO_BASE_URL",
        ("GLOO_PROCESS_ID", str(uuid.uuid4())),
        ("HOSTNAME", platform.node()),
        "BOUNDARY_PROJECT_ID",
        "BOUNDARY_SECRET",
        "OPENAI_API_KEY",
        ("GLOO_CAPTURE_CODE", "false"),
        ("GLOO_STAGE", "prod"),
        ("GLOO_CACHE", "0"),
    ]
)

__all__ = ["ENV"]
