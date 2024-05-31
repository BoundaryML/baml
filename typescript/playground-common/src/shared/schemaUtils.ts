// Based off Gloo-AI common utils

export type ObjType = {
  // baml output object
  value: any
}

export const parseGlooObject = (obj: ObjType) => {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  try {
    // Create a mock JSON string with the value and parse it
    // eslint-disable-next-line @typescript-eslint/restrict-template-expressions
    const mockJson = `{"tempKey": ${obj.value}}`
    const parsedMock = JSON.parse(mockJson)

    // Extract the value from the mock JSON
    // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-member-access
    return parsedMock.tempKey
  } catch (e) {
    // If parsing fails, return the original value
    // eslint-disable-next-line @typescript-eslint/no-unsafe-return
    return obj.value
  }
}
