require "async"
require_relative "../lib/baml"

start = Time.now

Async do |task|
  t = Baml::TokioDemo.new
  #task.async do
  #  puts "BEGIN- ruby-native sleep"
  #  sleep 1
  #  puts "END- ruby-native sleep"
  #end
  task.async do
    t.does_this_yield
  end
  task.async do
    t.does_this_yield
  end
  task.async do
    t.does_this_yield
  end
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

puts "Duration: #{Time.now - start}"
