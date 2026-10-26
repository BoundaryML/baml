# frozen_string_literal: true

module Baml
  class Handle
    def initialize(api, key, handle_type)
      @api = api
      @key = [key]
      @handle_type = handle_type
      @mutex = Mutex.new
      ObjectSpace.define_finalizer(self, self.class.finalizer(api, @key, Process.pid))
    end

    # The finalizer owns only the key cell, never the Ruby handle itself.
    def self.finalizer(api, key, owner_pid)
      proc do
        if Process.pid == owner_pid && key[0]
          owned = key[0]
          key[0] = nil
          begin
            api.handle_release(owned)
          rescue StandardError
            # Finalization cannot report a native cleanup failure to a caller.
          end
        end
      end
    end

    def close
      Thread.handle_interrupt(Exception => :never) do
        @mutex.synchronize do
          if @key[0]
            @api.handle_release(@key[0])
            @key[0] = nil
          end
        end
      end
      nil
    end

    def clone_for_wire(scope, borrowed: false)
      Thread.handle_interrupt(Exception => :never) do
        @mutex.synchronize do
          raise Bridge::Error, "BAML handle is closed" unless @key[0]

          key = @api.handle_clone(@key[0])
          borrowed ? scope.borrow(@api, key) : scope.own(@api, key)
          [key, @handle_type]
        end
      end
    end
  end

  class Stream < Handle
    extend T::Generic
    Value = type_member

    def next(_baml: nil)
      next_async(_baml: _baml).wait
    end

    def next_async(_baml: nil)
      Bridge.call_async("ai.stream.Stream.next", { "self" => self }, baml: _baml)
    end

    def final(_baml: nil)
      final_async(_baml: _baml).wait
    end

    def final_async(_baml: nil)
      Bridge.call_async("ai.stream.Stream.final", { "self" => self }, baml: _baml)
    end

    def each
      return enum_for(:each) unless block_given?

      # Resolve the generated sentinel through the registry, not an SDK namespace.
      done = Bridge::Protocol.registered_type("ai.stream.Done")
      loop do
        partial = self.next
        break if partial.is_a?(done)

        yield partial unless partial.nil?
      end
      self
    end
  end

  class FunctionSpec < Handle
    def build_request(client: nil, _baml: nil)
      build_request_async(client: client, _baml: _baml).wait
    end

    def build_request_async(client: nil, _baml: nil)
      Bridge.call_async("ai.FunctionSpec.build_request", { "self" => self, "client" => client }, baml: _baml)
    end
  end
  class Closure < Handle
    attr_reader :parameter_descriptors

    def initialize(api, key, handle_type, function_type = nil)
      @parameters = function_type&.params&.each_with_index&.map do |param, index|
        [param.has_name? ? param.name : "arg#{index}", param.mode == :BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL]
      end || []
      @parameter_descriptors = function_type&.params&.each_with_index&.to_h do |param, index|
        [@parameters[index][0], ::Baml::Types.descriptor(param.ty)]
      end || {}
      super(api, key, handle_type)
    end

    def call(*args, **kwargs)
      call_async(*args, **kwargs).wait
    end

    def call_async(*args, _baml: nil, **kwargs)
      required = @parameters.reject(&:last).map(&:first)
      raise ArgumentError, "Too many positional arguments for BAML closure" if args.length > required.length
      raise ArgumentError, "Specialized BAML closures do not accept _types" if kwargs.key?(:_types)

      values = required.zip(args).first(args.length).to_h
      kwargs.each do |name, value|
        wire = name.to_s
        raise ArgumentError, "Unknown BAML closure argument #{name}" unless @parameters.any? { |param, _| param == wire }
        raise ArgumentError, "Multiple values for BAML closure argument #{name}" if values.key?(wire)

        values[wire] = value
      end
      Bridge.call_async(self, values, baml: _baml)
    end
  end

  module Bridge
    class EncodingScope
      def initialize
        @handles = []
        @borrowed = []
      end

      def own(api, key)
        @handles << [api, key]
      end

      # Callable targets are borrowed, unlike argument handles: retain a clone
      # through native submission, then release it on either submission or rollback.
      def borrow(api, key)
        @borrowed << [api, key]
      end

      def submitted!
        @handles.clear
        close
      end

      def close
        handles = @handles + @borrowed
        @handles.clear
        @borrowed.clear
        handles.each { |api, key| api.handle_release(key) }
      end
    end
  end
end
