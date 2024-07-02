// How to use this BAML function in python or Typescript: https://docs.boundaryml.com/docs/snippets/supported-types#image
function DescribeImage(myImage: image) -> string {
  client GPT4o
  prompt #"
    {{ _.role("user") }}
    Describe this in 2 sentences: {{ myImage }}
  "#
}


test ImageTest {
  functions [DescribeImage]
  args {
    myImage { url "https://imgs.xkcd.com/comics/standards.png"}
  }
}
