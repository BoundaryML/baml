// Enum 
enum Sentiment {
  SAD
  HAPPY    
} 
 
enu Sentiment2 {  
  // comment
  ANGRY @alias("angry", {
    key "value"
  })
}     
 
enum BadEnum {   
  SAD 
  ANGRY
} 