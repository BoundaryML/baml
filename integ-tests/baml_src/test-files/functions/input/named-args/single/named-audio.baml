function AudioInput(aud: audio) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

   Does this sound like a dinosaur? Yes or no? One word no other characters.
    
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