// Void in function-type return position
type Callback = (int) -> void

// Higher-order function taking a void callback
function run_callback(cb: Callback, x: int) -> int {
  cb(x);
  x
}

// ERROR: assigning void callback result
function bad_callback_use(cb: Callback) -> int {
  let result = cb(1);
  1
}
