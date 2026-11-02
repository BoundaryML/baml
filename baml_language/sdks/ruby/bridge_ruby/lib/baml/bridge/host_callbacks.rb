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
          rescue Exception => error # rubocop:disable Lint/RescueException -- exceptions cannot cross FFI
            finish(frame, bridge_failure(error.message), EncodingScope.new) if frame
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
        completion = nil
        arguments = nil
        begin
          application = Wire::BamlToHostCall.decode(frame.application_args)
          values = Wire::BamlOutboundValue.new(list_value: { items: application.args.map(&:value) })
          arguments = Protocol::Result.new(nil, values, @api)
          callable, descriptor = HostValues.entry(frame.host_value_key)
          args = []
          kwargs = {}
          application.args.each do |arg|
            value = Protocol.decode_value(arg.value, @api, host_values: arguments.host_values, handles: arguments)
            arg.is_optional_arg ? kwargs[arg.arg_name.to_sym] = value : args << value
          end
          unless descriptor&.first == :function
            raise Error, "BAML callback has no function signature"
          end
          begin
            value = callable.call(*args, **kwargs)
            completion = encode_completion(value, descriptor[2], :returned, scope)
          rescue Exception => error # rubocop:disable Lint/RescueException -- includes host errors while reading the result
            completion = exception_completion(error, descriptor[3], scope)
          end
        rescue Exception => error # rubocop:disable Lint/RescueException -- infrastructure failures must complete too
          scope.close
          completion = bridge_failure(error.message)
        ensure
          arguments&.close
          finish(frame, completion, scope)
        end
      end

      # Reading may run getters or enumerators. Their exceptions leave this
      # method as host throws; once a plan is decided, failures are the bridge's.
      def encode_completion(value, slot, outcome, scope)
        begin
          tree = Encode::Read.new(Protocol).read(value, "result")
        rescue Encode::Refusal => error
          return refused_completion(error)
        end
        begin
          plan = Encode::Plan.new(Protocol).plan_root(tree, slot)
          Encode::Commit.check_depth!(Encode::Commit.depth(plan), "result", enclosing: 1)
        rescue Encode::Refusal => error
          return refused_completion(error)
        rescue Exception => error # rubocop:disable Lint/RescueException -- planning runs no host code
          return bridge_failure(error.message)
        end
        begin
          encoded = Encode::Commit.commit(plan, scope: scope)
          Encode::Commit.check_depth!(Encode::Slot.wire_depth(encoded), "result", enclosing: 1)
          Wire::HostCallCompletion.new(**{ outcome => encoded })
        rescue Exception => error # rubocop:disable Lint/RescueException -- commit failures are never host throws
          scope.close
          bridge_failure(error.message)
        end
      end

      def refused_completion(error)
        if error.is_a?(Encode::Fault) || error.is_a?(Encode::Ambiguous) ||
           error.is_a?(Encode::MissingTypeArgs) || error.is_a?(Encode::UnboundTypeArgs) || error.is_a?(Encode::UntypedCallable)
          bridge_failure(error.message)
        else
          Wire::HostCallCompletion.new(contract_violation: error.message)
        end
      end

      def exception_completion(error, throws, scope)
        if error.is_a?(::Baml::Error) || error.is_a?(::Baml::Panic)
          slot = error.is_a?(::Baml::Panic) ? [:unknown] : throws
          begin
            return encode_completion(error.value, slot, :thrown, scope)
          rescue Exception => reading_error # rubocop:disable Lint/RescueException -- a thrown value's getter may also raise
            error = reading_error
          end
        end
        key = HostValues.register(error, scope)
        fields = { message: utf8(error.message), class_name: error.class.name || error.class.to_s, language: "ruby", handle: key }
        fields[:traceback] = utf8(error.backtrace.join("\n")) if error.backtrace
        Wire::HostCallCompletion.new(raised: Wire::HostException.new(**fields))
      end

      def utf8(text)
        text = text.dup.force_encoding(Encoding::UTF_8) if text.encoding == Encoding::BINARY
        text.encode(Encoding::UTF_8, invalid: :replace, undef: :replace)
      end

      def bridge_failure(message)
        Wire::HostCallCompletion.new(bridge_failure: message)
      end

      def finish(frame, completion, scope)
        Thread.handle_interrupt(Exception => :never) do
          begin
            @api.complete_host_call(frame.callback_id, Wire::HostCallCompletion.encode(completion))
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
