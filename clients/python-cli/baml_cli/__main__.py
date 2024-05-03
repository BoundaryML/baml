import argparse
import baml_py


def main():
    parser = argparse.ArgumentParser(prog='baml_cli', description="BAML CLI, Python style")
    subparsers = parser.add_subparsers(dest="command")

    generate_parser = subparsers.add_parser("generate", help="Generate a client")
    generate_parser.add_argument(
        "--from",
        dest='baml_src',
        type=str,
        required=True,
        help="baml_src directory to generate from",
    )
    generate_parser.add_argument(
        "--to",
        dest='output_path',
        type=str,
        required=True,
        help="directory to write the generated baml_client/ to",
    )
    generate_parser.add_argument(
        "--type",
        dest='client_type',
        type=str,
        required=False,
        choices=["python/pydantic", "ruby"],
        help="type of client to generate (defaults to 'python/pydantic')",
    )

    args = parser.parse_args()

    if args.command == "generate":
        print(
            f"Generating from {args.baml_src} to {args.output_path} with type {args.client_type}"
        )
        runtime = baml_py.BamlRuntimeFfi.from_directory(args.baml_src)
        runtime.generate_client(
            baml_py.GenerateArgs.new(
                client_type=args.client_type,
                output_path=args.output_path,
            )
        )
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
