function DescribeImage(img: image) -> string {
  client AwsBedrock
  prompt #"
    {{ _.role("user") }}


    Describe the image below in 20 words:
    {{ img }}
  "#

}

class FakeImage {
  url string
}

class ClassWithImage {
  myImage image
  param2 string
  fake_image FakeImage
}

// chat role user present
function DescribeImage2(classWithImage: ClassWithImage, img2: image) -> string { 
  client GPT4Turbo
  prompt #"
    {{ _.role("user") }}
    You should return 2 answers that answer the following commands.

    1. Describe this in 5 words:
    {{ classWithImage.myImage }}

    2. Also tell me what's happening here in one sentence:
    {{ img2 }}
  "#
}

// no chat role
function DescribeImage3(classWithImage: ClassWithImage, img2: image) -> string {
  client GPT4Turbo
  prompt #"
    Describe this in 5 words:
    {{ classWithImage.myImage }}

    Tell me also what's happening here in one sentence and relate it to the word {{ classWithImage.param2 }}:
    {{ img2 }}
  "#
}


// system prompt and chat prompt
function DescribeImage4(classWithImage: ClassWithImage, img2: image) -> string {
  client GPT4Turbo
  prompt #"
    {{ _.role("system")}}

    Describe this in 5 words:
    {{ classWithImage.myImage }}

    Tell me also what's happening here in one sentence and relate it to the word {{ classWithImage.param2 }}:
    {{ img2 }}
  "#
}

test TestName {
  functions [DescribeImage]
  args {
    img { url "https://imgs.xkcd.com/comics/standards.png"}
  }
}
