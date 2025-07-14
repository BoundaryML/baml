function DescribeAudio(audio: audio) -> string {
  client GPT4o
  prompt #"
    Describe the audio below in 20 words:
    {{ _.role("user") }}
    {{ audio }}
  "#

}




// chat role user present
function DescribeAudio2(audio: audio) -> string {
  client GPT4Turbo
  prompt #"
    {{ _.role("user") }}
    You should return 1 answer that answer the following command.

    Describe this in 5 words:
    {{ audio }}
  "#
}

test TestAudio {
    functions [DescribeAudio]
  args {
    audio { url "https://www.pacdv.com/sounds/voices/friday-rocks.wav"}
  }
}

test TestAudio2 {
  functions [DescribeAudio2]
  args {
    audio { file "friday-rocks.wav" }
      }
}
