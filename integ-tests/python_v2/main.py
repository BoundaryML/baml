import asyncio
import baml_py
from baml_py import Image
from pydantic import BaseModel
import time


class FakeImage(BaseModel):
    url: str


class ClassWithImage(BaseModel):
    myImage: Image
    param2: str
    fake_image: FakeImage


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

   

    


start_time = time.perf_counter()

asyncio.run(main())

end_time = time.perf_counter()
elapsed_time = end_time - start_time
print(f"Elapsed time: {elapsed_time:.2f} seconds")
