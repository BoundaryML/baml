// Complex string manipulation test

function ProcessEmail() -> string {
  let email = "  User@EXAMPLE.COM  ";
  let cleaned = email.trim().toLowerCase();
  let parts = cleaned.split("@");
  let domain = parts[1];
  domain.substring(0, 7)
}

//> ProcessEmail()
//
// "example"

function ValidateMessage() -> bool {
  let msg = "Hello World";
  let hasHello = msg.includes("Hello");
  let startsRight = msg.startsWith("Hello");
  let endsRight = msg.endsWith("World");
  let rightLength = msg.length() == 11;
  hasHello && startsRight && endsRight && rightLength
}

//> ValidateMessage()
//
// true
