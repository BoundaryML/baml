function AudioInput(aud: audio) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

   Does this sound like a dinosaur? Yes or no? Answer with a single word and no other characters.
    
    {{ aud }}
  "#
}


test TestURLAudioInput{
  functions [AudioInput]
  args {
    aud{ 
      media_type mp4
      url http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4
    }
  }  
}