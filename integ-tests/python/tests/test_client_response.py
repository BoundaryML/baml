# from baml_py import Collector
# from baml_client import b

# First run the http server
# uv run http-server.py

# import pytest
# @pytest.mark.asyncio
# async def test_client_response():
#     res = await b.OpenAIWithAnthropicResponseHello("unused")
#     # assert res == "Hello, world!"
#     print(res)


# @pytest.mark.asyncio
# async def test_custom_client_response():
#     collector = Collector(name="my-collector")
#     res = await b.TestOpenAIDummyClient("unused", baml_options={"collector": collector})
#     # assert res == "Hello, world!"
#     print(res)
#     logs = collector.logs
#     assert len(logs) == 1
#     assert logs[0].function_name == "TestOpenAIDummyClient"
#     assert logs[0].log_type == "call"

#     call = logs[0].calls[0]
#     call.http_request

#     response = call.http_response
#     assert response is not None
#     assert response.status == 200
#     assert response.body is not None
#     assert isinstance(response.body, dict)
#     assert response.body["choices"][0]["logprobs"]["content"][0]["token"] == " Yes"
#     assert (
#         response.body["choices"][0]["logprobs"]["content"][0]["logprob"]
#         == -0.034360505640506744
#     )
#     assert response.body["choices"][0]["logprobs"]["content"][1]["token"] == ""
#     print(call.http_response)
