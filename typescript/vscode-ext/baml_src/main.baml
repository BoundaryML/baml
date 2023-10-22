// Enum 
enum Sentiment {
  SAD
  HAPPY    
} 
 
enum Sentiment2 {  
  // comment
  ANGRY @alias("angry", {
    key "value"
  })
  SAD @alias(#"
    block string here
    yes
  "#)
}     
 
enum BadEnum {   
  SAD 
  ANGRY
} 