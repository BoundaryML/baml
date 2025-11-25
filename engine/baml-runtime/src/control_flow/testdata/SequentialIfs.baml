function SequentialIfs() -> int {
  //# Setup context
  {
    let initialize = 5;
    //# In-scope configuration
    let configure = 10;
  }
  if (false) {
    //# listen to SQS for S3
    let listenToSqs = 30;
    //# listen to SQS for EC2
    let listenToSqsForEc2 = 30;
  } else if (true) {
    //## listen to Kinesis
    let listenToKinesis = 35;
  } else {
    let k = 5;
  }
  if (false) {
    //# listen to SQS
    let listenToSqs = 30;
  } else {
    let k = 5;
  }
  //# Done
  let finish = 35;
  finish
}