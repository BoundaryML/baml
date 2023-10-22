def {{name}}Impl(request: FixtureRequest) -> I{{name}}:
    """
    To use this fixture, add this to your test.
    Note the parameter name must match the name of this fixture.

    ```python
    @baml.{{name}}.test
    async def test_logic({{name}}Impl: I{{name}}) -> None:
        result = await {{name}}Impl(args_here)
        ...
    ```

    See the docstring for baml.{{name}}.test for more information.


    See pytest documentation for more information on fixtures:
    https://docs.pytest.org/en/latest/fixture.html
    """
    return baml.{{name}}.get_impl(request.param).run

