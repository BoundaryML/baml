function TestImageListInput(imgs: image[]) -> string{
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    What colors do these have in common? {{imgs}}
  "#
}

test  TestImageListInput {
  functions [TestImageListInput]
  args {
    imgs [
      {
        media_type "image/png"
        url "https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png"
      },
      {
        url "https://i.imgur.com/93fWs5R.png"
      }
    ]
  }
}
