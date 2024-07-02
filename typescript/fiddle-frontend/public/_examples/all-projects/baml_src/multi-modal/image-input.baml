// BAML supports multi-modal inputs! Check the raw cURL request toggle in the playground to see the prompt is transformed into an API call

// How to use call this BAML function in python or Typescript: https://docs.boundaryml.com/docs/snippets/supported-types#image

class ImageDescription {
  description string
  tags string[] @description(#"
    Tags that describe the image
  "#)
}

function DescribeImage(myImage: image) -> ImageDescription {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    Describe this in 2 sentences: {{ myImage }}

    {{ ctx.output_format }}
  "#
}


test ImageTest {
  functions [DescribeImage]
  args {
    myImage { url "https://imgs.xkcd.com/comics/standards.png"}
  }
}
