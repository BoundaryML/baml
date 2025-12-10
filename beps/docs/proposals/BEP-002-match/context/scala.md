# Scala Match Syntax

Scala's `match` expression is a fundamental part of the language, deeply integrated with case classes and sealed traits.

## Basic Syntax

```scala
val x = 1
val result = x match {
  case 1 => "one"
  case 2 => "two"
  case _ => "other"
}
```

## Key Features

### 1. Case Classes and Destructuring

Scala's case classes are designed for pattern matching.

```scala
sealed trait Notification
case class Email(sender: String, title: String, body: String) extends Notification
case class SMS(caller: String, message: String) extends Notification
case class VoiceRecording(contactName: String, link: String) extends Notification

def showNotification(notification: Notification): String = {
  notification match {
    case Email(sender, title, _) =>
      s"You got an email from $sender with title: $title"
    case SMS(number, message) =>
      s"You got an SMS from $number! Message: $message"
    case VoiceRecording(name, link) =>
      s"You received a Voice Recording from $name! Click the link to hear it: $link"
  }
}
```

### 2. Guards

You can add `if` guards to cases.

```scala
case Email(sender, _, _) if sender == "boss@work.com" => "Important!"
```

### 3. Type Matching

You can match on types.

```scala
def go(device: Device) = {
  device match {
    case p: Phone => p.screenOff
    case c: Computer => c.screenSaverOn
  }
}
```

### 4. Sealed Traits and Exhaustiveness

If you match on a `sealed trait`, the compiler checks for exhaustiveness.

```scala
sealed trait Answer
case object Yes extends Answer
case object No extends Answer

// Warning: match may not be exhaustive. It would fail on the following input: No
val x: Answer = Yes
x match {
  case Yes => println("Yes")
}
```

### 5. Regex Matching

Scala allows matching with Regex.

```scala
val date = "2023-01-01"
val dateRegex = """(\d{4})-(\d{2})-(\d{2})""".r

date match {
  case dateRegex(year, month, day) => s"$year was a good year"
  case _ => "Not a date"
}
```
