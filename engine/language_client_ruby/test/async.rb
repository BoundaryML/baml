require "async"
require_relative "../lib/baml"

start = Time.now

Async do |task|
  #task.async do
  #  puts "BEGIN- ruby-native sleep"
  #  sleep 1
  #  puts "END- ruby-native sleep"
  #end
  task.async do
    Fiber.schedule do
      Baml.hello_from_rust()
    end
  end
  task.async do
    Fiber.schedule do
      Baml.hello_from_rust()
    end
  end
  task.async do
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
  end
end

puts "Duration: #{Time.now - start}"
