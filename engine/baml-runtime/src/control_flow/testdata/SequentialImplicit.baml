function SequentialImplicit() -> int {
  //# Setup context
  {
    let initialize = 5;
    //# In-scope configuration
    let configure = 10;
  }
  while (true) {
    //# wait for feature flags
    let featureFlags = 15;
  }
  if (false) {
    //# listen to SQS
    let listenToSqs = 30;
  } else {
    let k = 5;
  }
  {
    let startDatabase = 20;
    //# Bootstrap schemas
    let bootstrapSchemas = 25;
  }
  //# Done
  let finish = 35;
  finish
}