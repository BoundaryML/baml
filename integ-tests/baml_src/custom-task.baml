class BookOrder {
  orderId string @description(#"
    The ID of the book order
  "#)
  title string @description(#"
    The title of the ordered book
  "#)
  quantity int @description(#"
    The quantity of books ordered
  "#)
  price float @description(#"
    The price of the book
  "#)
}

class FlightConfirmation {
  confirmationNumber string @description(#"
    The flight confirmation number
  "#)
  flightNumber string @description(#"
    The flight number
  "#)
  departureTime string @description(#"
    The scheduled departure time of the flight
  "#)
  arrivalTime string @description(#"
    The scheduled arrival time of the flight
  "#)
  seatNumber string @description(#"
    The seat number assigned on the flight
  "#)
}

class GroceryReceipt {
  receiptId string @description(#"
    The ID of the grocery receipt
  "#)
  storeName string @description(#"
    The name of the grocery store
  "#)
  items (string | int | float)[] @description(#"
    A list of items purchased. Each item consists of a name, quantity, and price.
  "#)
  totalAmount float @description(#"
    The total amount spent on groceries
  "#)
}

class CustomTaskResult {
  bookOrder BookOrder | null
  flightConfirmation FlightConfirmation | null
  groceryReceipt GroceryReceipt | null
}

function CustomTask(input: string) -> BookOrder | FlightConfirmation | GroceryReceipt {
  client "openai/gpt-4o-mini"
  prompt #"
    Given the input string, extract either an order for a book, a flight confirmation, or a grocery receipt.

    {{ ctx.output_format }}

    Input:
    
    {{ input}}
  "#
}

test CustomTask {
  functions [CustomTask]
  args {
    input #"
Dear [Your Name],

Thank you for booking with [Airline Name]! We are pleased to confirm your upcoming flight.

Flight Confirmation Details:

Booking Reference: ABC123
Passenger Name: [Your Name]
Flight Number: XY789
Departure Date: September 15, 2024
Departure Time: 10:30 AM
Arrival Time: 1:45 PM
Departure Airport: John F. Kennedy International Airport (JFK), New York, NY
Arrival Airport: Los Angeles International Airport (LAX), Los Angeles, CA
Seat Number: 12A
Class: Economy
Baggage Allowance:

Checked Baggage: 1 piece, up to 23 kg
Carry-On Baggage: 1 piece, up to 7 kg
Important Information:

Please arrive at the airport at least 2 hours before your scheduled departure.
Check-in online via our website or mobile app to save time at the airport.
Ensure that your identification documents are up to date and match the name on your booking.
Contact Us:

If you have any questions or need to make changes to your booking, please contact our customer service team at 1-800-123-4567 or email us at support@[airline].com.

We wish you a pleasant journey and thank you for choosing [Airline Name].

Best regards,

[Airline Name] Customer Service
    "#
  }
}