function DescribeImage(img: image) -> string {
  client GPT4Turbo
  prompt #"
    {{ _.chat(role="user") }}


    Describe the image below in 5 words:
    {{ img }}
  "#

}
// Open main.py to see how to use this function