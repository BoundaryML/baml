import asyncio
import baml_py
#from baml_py import Image
#from pydantic import BaseModel
import time
#from baml_client import client
#from baml_client.types import ClassWithImage, FakeImage

def throwing_cb(d):
    raise Exception("oops")

async def fetch_data(url: str):
    print(f"Fetching data from {url}...")
    await asyncio.sleep(1)
    print(f"Received data from {url}!")
    return f"Data from {url}"



async def main():
    b = baml_py.BamlRuntimeFfi.from_directory("../../integ-tests/baml_src")
    spongebob_image = Image(
        url="https://i.kym-cdn.com/photos/images/original/002/807/304/a0b.jpeg"
    )
    print("image", spongebob_image)

    print("image.url", spongebob_image.url)

    orc_image = Image(
        url="https://i.kym-cdn.com/entries/icons/original/000/033/100/eht0m1qg8dk21.jpg"
    )

    full_obj = ClassWithImage(
        myImage=spongebob_image,
        param2="ocean",
        fake_image=FakeImage(
            url="https://i.kym-cdn.com/entries/icons/original/000/033/100/eht0m1qg8dk21.jpg"
        ),
    )

    res = await b.call_function(
        "DescribeImage2", args={"classWithImage": full_obj, "img2": orc_image}, ctx={}
    )
    print("res-------\n", res)

    runtime = client.BamlClient.from_directory("../../integ-tests/baml_src")
    res2 = await runtime.DescribeImage2(classWithImage=full_obj, img2=orc_image)
    print("res2-------\n", res2)

async def main2():
    b = baml_py.BamlRuntimeFfi.from_directory("/home/sam/repos/baml-examples/nextjs-starter-v1/baml_src", {})

    #retval = await b.stream(lambda d, e, f: print(f"<in python cb>cb arg: {f}</in python cb>"))
    print("starting stream")
    stream = b.stream_function("ExtractResume", {"raw_text": "john doe is a skilled carpenter in the Dada style"}, ctx={}, on_event=lambda d: print(f"<in py-cb1>cb arg: {d}</in py-cb>"))
    stream2 = b.stream_function("ExtractResume", {"raw_text": "jane smith is a skilled carpenter in the Dada style"}, ctx={}, on_event=throwing_cb)
    retval = await asyncio.gather(
        #asyncio.wait_for(stream.on_event(lambda d: print(f"<on-event>{d}</on-event>")).done(), timeout=1.7),
        stream.on_event(lambda d: print(f"<on-event>{d}</on-event>")).done(),
        stream2.done())
    print("ending stream")

    print("retval", retval[0], retval[1])
    

async def main3():

    # simple form
    await (b.stream.ExtractResumes(text='asdf')
        .withEventHandlers()
        .done())
    
    # more complex form
    stream_manager = b.stream.ExtractResumes(text='asdf')
    stream_manager = stream_manager.withEventHandlers(...)

    stream = await stream_manager.start()
    final = await stream.end()





start_time = time.perf_counter()

asyncio.run(main2())

end_time = time.perf_counter()
elapsed_time = end_time - start_time
print(f"Elapsed time: {elapsed_time:.2f} seconds")
