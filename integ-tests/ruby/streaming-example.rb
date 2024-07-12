require_relative "baml_client/client"

$b = Baml.Client

# Using both iteration and get_final_response() from a stream
def example1(receipt)
  stream = $b.stream.ExtractReceiptInfo(receipt)

  stream.each do |partial|
    puts "partial: #{partial.items&.length} items"
  end

  final = stream.get_final_response
  puts "final: #{final.items.length} items"
end

# Using only iteration of a stream
def example2(receipt)
  $b.stream.ExtractReceiptInfo(receipt).each do |partial|
    puts "partial: #{partial.items&.length} items"
  end
end

# Using only get_final_response() of a stream
#
# In this case, you should just use BamlClient.ExtractReceiptInfo(receipt) instead,
# which is faster and more efficient.
def example3(receipt)
  final = $b.stream.ExtractReceiptInfo(receipt).get_final_response
  puts "final: #{final.items.length} items"
end

receipt = <<~RECEIPT
  04/14/2024 1:05 pm

  Ticket: 220000082489
  Register: Shop Counter
  Employee: Connor
  Customer: Sam
  Item  #  Price
  Guide leash (1 Pair) uni UNI
  1 $34.95
  The Index Town Walls
  1 $35.00
  Boot Punch
  3 $60.00
  Subtotal $129.95
  Tax ($129.95 @ 9%) $11.70
  Total Tax $11.70
  Total $141.65
RECEIPT

if __FILE__ == $0
  example1(receipt)
  example2(receipt)
  example3(receipt)
end
