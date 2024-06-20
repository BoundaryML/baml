function AudioInput(aud: audio) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

   What is this person? Answer with a single word and no other characters.
    
    {{ aud }}
  "#
}


test TestURLAudioInput{
  functions [AudioInput]
  args {
    aud{ 
      url http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerMeltdowns.mp4
    }
  }  
}