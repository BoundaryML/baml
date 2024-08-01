
module Baml
  module Tracing1
    def self.included(base)
      base.extend(ClassMethods)
    end

    module ClassMethods
      def trace
        @trace_next_method = true
      end

      def method_added(method_name)
        super
        return unless @trace_next_method

        @trace_next_method = false
        original_method = instance_method(method_name)

        define_method(method_name) do |*args, &block|
          start_time = Time.now
          result = original_method.bind(self).call(*args, &block)
          end_time = Time.now
          
          duration = (end_time - start_time) * 1000 # Convert to milliseconds
          puts "Method #{method_name} took #{duration.round(2)}ms to execute"
          
          result
        end
      end
    end
  end
end

class MyController1
  include Baml::Tracing1

  def show
    puts "show"
  end

  private

  trace
  def authorize_user
    puts "authorize_user"
  end

  trace
  def find_post
    puts "find_post"
  end
end

c = MyController1.new
c.show
c.authorize_user
c.find_post

module Baml
  module Tracing2
    def self.included(base)
      base.extend(ClassMethods)
    end

    module ClassMethods
      def trace(*method_names)
        method_names.each do |method_name|
          original_method = instance_method(method_name)
          define_method(method_name) do |*args, &block|
            start_time = Time.now
            result = original_method.bind(self).call(*args, &block)
            end_time = Time.now
            
            duration = (end_time - start_time) * 1000 # Convert to milliseconds
            
            log_trace(method_name, duration)
            
            result
          end
        end
      end
    end

    private

    def log_trace(method_name, duration)
      puts "[TRACE] #{self.class}##{method_name} - #{duration.round(2)}ms"
    end
  end
end

class MyController2
  include Baml::Tracing2

  def show
    puts "show"
  end

  private

  trace
  def authorize_user
    puts "authorize_user"
  end

  trace
  def find_post
    puts "find_post"
  end
end

c = MyController1.new
c.show
c.authorize_user
c.find_post