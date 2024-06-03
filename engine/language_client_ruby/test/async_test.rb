require "async"
require_relative "../lib/baml"

start = Time.now

#t = Baml::Ffi::TokioDemo.new

#puts "BEGIN sync sleep"
#t.does_this_yield
#t.does_this_yield
#puts "END sync sleep"
#puts

puts "BEGIN async sleep"
#Async do |task|
#
#  Fiber.schedule do
#    t.does_this_yield
#  end
#  Fiber.schedule do
#    t.does_this_yield
#  end
#  Fiber.schedule do
#    t.does_this_yield
#  end
#  Fiber.schedule do
#    t.does_this_yield
#  end
#  #task.async do
#  #  t.does_this_yield
#  #end
#end
puts "END async sleep"

#t.shutdown

puts "Duration: #{Time.now - start}"
