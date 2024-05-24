import asyncio
import time
from baml_client import b
from baml_client.tracing import trace

@trace
async def main2():
    # single = await b.ExtractResume(resume="Lee Hsien Loong[a] SPMJ DK (born 10 February 1952) is a Singaporean politician and former brigadier-general who has been serving as Senior Minister of Singapore since 2024, having previously served as Prime Minister of Singapore from 2004 to 2024. He has been the Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991, and previously Teck Ghee SMC between 1984 and 1991, as well as Secretary-General of the People's Action Party (PAP) since 2004.")
    # print(single)
    # return
    # retval = await b.stream(lambda d, e, f: print(f"<in python cb>cb arg: {f}</in python cb>"))
    print("starting stream")
    # stream = b.stream.ExtractResume(resume="Lee Hsien Loong[a] SPMJ DK (born 10 February 1952) is a Singaporean politician and former brigadier-general who has been serving as Senior Minister of Singapore since 2024, having previously served as Prime Minister of Singapore from 2004 to 2024. He has been the Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991, and previously Teck Ghee SMC between 1984 and 1991, as well as Secretary-General of the People's Action Party (PAP) since 2004.")
    stream = b.stream.ExtractNames(
        input="We would like to thank Qianqian Wang, Justin Kerr, Brent Yi, David McAllister, Matthew Tancik, Evonne Ng, Anjali Thakrar, Christian Foley, Abhishek Kar, Georgios Pavlakos, the Nerfstudio team, and the KAIR lab for discussions, feedback, and technical support. We also thank Ian Mitchell and Roland Jose for helping to label points."
    )

    async for event in stream:
        print(f"baml-async-for: {event}")

    final_message = await stream.done()

    print("baml-final-message:", final_message)


start_time = time.perf_counter()

asyncio.run(main2())

end_time = time.perf_counter()
elapsed_time = end_time - start_time
print(f"Elapsed time: {elapsed_time:.2f} seconds")
