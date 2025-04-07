from ..baml_client import partial_types
from ..baml_client.types import LinkedList, Node
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b


def test_parse_llm_response():
    llm_response = """
        ```json
        {
            "len": 5,
            "head": {
                "data": 1,
                "next": {
                    "data": 2,
                    "next": {
                        "data": 3,
                        "next": {
                            "data": 4,
                            "next": {
                                "data": 5,
                                "next": null
                            }
                        }
                    }
                }
            }
        }
        ```
    """

    parsed = b.parse.BuildLinkedList(llm_response)

    assert parsed == LinkedList(
        len=5,
        head=Node(
            data=1,
            next=Node(
                data=2,
                next=Node(data=3, next=Node(data=4, next=Node(data=5, next=None))),
            ),
        ),
    )


def test_parse_llm_response_sync():
    llm_response = """
        ```json
        {
            "len": 5,
            "head": {
                "data": 1,
                "next": {
                    "data": 2,
                    "next": {
                        "data": 3,
                        "next": {
                            "data": 4,
                            "next": {
                                "data": 5,
                                "next": null
                            }
                        }
                    }
                }
            }
        }
        ```
    """

    parsed = sync_b.parse.BuildLinkedList(llm_response)

    assert parsed == LinkedList(
        len=5,
        head=Node(
            data=1,
            next=Node(
                data=2,
                next=Node(data=3, next=Node(data=4, next=Node(data=5, next=None))),
            ),
        ),
    )


def test_parse_llm_stream():
    stream = """
        ```json
        {
            "name": "John Doe",
            "email": "john.doe@example.com",
        ```
    """

    parsed = b.parse_stream.ExtractResume(stream)

    assert parsed == partial_types.Resume(
        name="John Doe",
        email="john.doe@example.com",
        phone=None,
        experience=[],
        education=[],
        skills=[],
    )


def test_parse_llm_stream_sync():
    stream = """
        ```json
        {
            "name": "John Doe",
            "email": "john.doe@example.com",
        ```
    """

    parsed = sync_b.parse_stream.ExtractResume(stream)

    assert parsed == partial_types.Resume(
        name="John Doe",
        email="john.doe@example.com",
        phone=None,
        experience=[],
        education=[],
        skills=[],
    )
