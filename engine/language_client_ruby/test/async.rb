require "async"
require_relative "../lib/baml"

start = Time.now

#Async do |task|
  #task.async do
  #  puts "BEGIN- ruby-native sleep"
  #  sleep 1
  #  puts "END- ruby-native sleep"
  #end
  #task.async do
  #    Baml.hello_from_rust()
  #end
  #task.async do
  #    Baml.hello_from_rust()
  #end
  #task.async do
  #  Fiber.schedule do
  #    puts "a-put1"
  #    #Fiber.yield
  #    puts "a-put2"
  #  end
  #  Fiber.schedule do
  #    puts "b-put1"
  #    #Fiber.yield
  #    puts "b-put2"
  #  end
#  end
#end

# Create an array to store the Ractors
ractors = []

# Create 10,000 Ractors
100.times do
  ractors << does_this_yield()
end

# Wait on all Ractors to finish
ractors.each(&:take)

puts "Duration: #{Time.now - start}"
