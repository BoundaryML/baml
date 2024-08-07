// function AudioInput(aud: audio) -> string{
//   client Gemini
//   prompt #"
//     {{ _.role("user") }}

//    Does this sound like a roar? Yes or no? One word no other characters.
    
//     {{ aud }}
//   "#
// }



// test TestURLAudioInput{
//   functions [AudioInput]
//   args {
//     aud{ 
//       url https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg
//     }
//   }  
// }


enum CatA {
  A
}

enum CatB {
  C
  D
}

class CatAPicker {
  cat CatA
}

class CatBPicker {
  cat CatB
  item int
}

enum CatC {
  E
  F 
  G 
  H 
  I
}

class CatCPicker {
  cat CatC
  item  int | string | null
  data int?
}