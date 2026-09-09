"""Check the generated explicit host binder's accepted and rejected bodies."""

from check_input_roles import main

if __name__ == "__main__":
    main([
        ("generated_host_bindings.py", False),
        ("generated_host_bindings_rejections.py", True),
    ])
