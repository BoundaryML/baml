import asyncio

class CallbackOnTimer:
  def __init__(self, cb):
    self.__callback = cb
  
  async def done(self):
    for i in range(5):
      self.__callback(i)
      await asyncio.sleep(1)
    return "done"


class AsyncIterableAPI:
    def __init__(self):
        self.queue = asyncio.Queue()
        self.callback_on_timer = CallbackOnTimer(self.__enqueue)

    def __enqueue(self, data):
        self.queue.put_nowait((False, data))

    async def __drive_to_completion(self):
      fin = await self.callback_on_timer.done()
      self.queue.put_nowait((True, fin))

    async def __aiter__(self):
        final_resp = asyncio.create_task(self.__drive_to_completion())
        while True:
            is_done, data = await self.queue.get()
            if is_done:
               break
            yield data
        yield final_resp

async def main():
  a = CallbackOnTimer(lambda i: print(f"callback called {i}"))
  b = await a.done()
  print("CallbackOnTimer done", b)

  async for i in AsyncIterableAPI():
    print(f"got {i} via await")


asyncio.run(main())