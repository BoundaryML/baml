// declaration
interface /* keyword */ Named /* name */ <T> /* generics */ requires /* requires */ Display /* target */, /* comma */ Identity /* header */ { // open
// associated
 type Key extends string = string; // alias
 name: string; // field
 // method
 function label(self) -> string // signature
 function value(self) -> T throws never // throws
 // default
 function fallback(self) -> string throws never { "fallback" } // body
 // close
} // end
