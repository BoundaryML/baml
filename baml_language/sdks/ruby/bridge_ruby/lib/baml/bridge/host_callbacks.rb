# frozen_string_literal: true

module Baml
  module Bridge
    # Host keys are borrowed on the outbound wire. Only the engine's release
    # callback (or rollback of an unsubmitted encoding) removes their owners.
    module HostValues
      @mutex = Mutex.new
      @values = {}
      @next_key = 1

      def self.register(value, scope, descriptor = nil)
        raise Error, "BAML host values can only be sent inside an encoding scope" unless scope

        Thread.handle_interrupt(Exception => :never) do
          @mutex.synchronize do
            raise Error, "BAML host-value keys are exhausted" if @next_key > 0xffff_ffff_ffff_ffff

            key = @next_key
            @next_key += 1
            @values[key] = [value, descriptor].freeze
            scope.own(self, key)
            key
          end
        end
      end

      def self.fetch(key)
        entry(key).first
      end

      def self.entry(key)
        @mutex.synchronize { @values.fetch(key) { raise Error, "Unknown BAML host-value key #{key}" } }
      end

      def self.handle_release(key)
        @mutex.synchronize { @values.delete(key) }
        nil
      end
    end

    class HostCallbacks
      Wire = Protocol::Wire

      def initialize(api)
        @api = api
        @dispatch = FFI::Function.new(:void, %i[pointer size_t]) do |pointer, length|
          frame = nil
          begin
            # The engine lends these bytes only until this callback returns.
            frame = Wire::HostInvocation.decode(pointer.read_bytes(length))
            Thread.new { invoke(frame) }
          rescue Exception # rubocop:disable Lint/RescueException -- exceptions cannot cross FFI
            finish(frame, 1, "".b, EncodingScope.new) if frame
          end
        end
        @release = FFI::Function.new(:void, [:uint64]) { |key| HostValues.handle_release(key) }
        # Ruby callbacks cancel cooperatively: never kill their thread or drop
        # their resources here. complete_host_call reports actual host exit.
        @cancel = FFI::Function.new(:void, [:uint32]) { |_id| }
      end

      def register!
        @api.register_host_callbacks(@dispatch, @release, @cancel)
      end

      private

      def invoke(frame)
        scope = EncodingScope.new
        is_error = 0
        bytes = "".b
        begin
          callable, descriptor = HostValues.entry(frame.host_value_key)
          args = []
          kwargs = {}
          Wire::BamlToHostCall.decode(frame.application_args).args.each do |arg|
            value = Protocol.decode_value(arg.value, @api)
            arg.is_optional_arg ? kwargs[arg.arg_name.to_sym] = value : args << value
          end
          value = callable.call(*args, **kwargs)
          returns = Protocol.encoding_hint(descriptor, :function, 2)
          bytes = Wire::InboundValue.encode(Protocol.encode_value(value, returns, scope: scope))
        rescue Exception => error # rubocop:disable Lint/RescueException -- all host throws must complete
          scope.close
          scope = EncodingScope.new
          is_error = 1
          begin
            bytes = Wire::InboundValue.encode(Protocol.encode_host_error(error, scope: scope))
          rescue Exception # rubocop:disable Lint/RescueException -- empty error means bridge failure
            bytes = "".b
          end
        ensure
          finish(frame, is_error, bytes, scope)
        end
      end

      def finish(frame, is_error, bytes, scope)
        Thread.handle_interrupt(Exception => :never) do
          begin
            @api.complete_host_call(frame.callback_id, is_error, bytes)
            scope.submitted!
          ensure
            begin
              scope.close
            ensure
              begin
                @api.handle_release(frame.effective_state) unless frame.effective_state.zero?
              ensure
                Protocol.release_outbound_references(frame.cancel, @api)
              end
            end
          end
        end
      end
    end
  end
end
