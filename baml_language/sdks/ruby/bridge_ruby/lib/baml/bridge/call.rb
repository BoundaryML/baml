# frozen_string_literal: true

require "sorbet-runtime"
require "thread"

module Baml
  class WaitTimeout < StandardError; end

  class Call
    extend T::Generic
    Value = type_member

    def initialize(api, call_id)
      @api = api
      @call_id = call_id
      @mutex = Mutex.new
      @condition = ConditionVariable.new
      @state = :pending
      @decoded = false
      @cancel_requested = false
    end

    def wait(timeout: nil)
      deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + timeout if timeout
      @mutex.synchronize do
        while @state == :pending
          remaining = deadline - Process.clock_gettime(Process::CLOCK_MONOTONIC) if deadline
          raise WaitTimeout, "BAML call is still running" if remaining && remaining <= 0

          @condition.wait(@mutex, remaining)
        end
        # A waiter's interrupt must not become the shared decoding error.
        Thread.handle_interrupt(Exception => :never) do
          unless @decoded
            begin
              raise @outcome if @outcome.is_a?(Exception)

              @value = Bridge::Protocol.decode_result(@outcome, @api)
            rescue Exception => error # rubocop:disable Lint/RescueException -- cache the engine outcome
              @error = error
            end
            @decoded = true
            @outcome = nil
          end
        end
        raise @error if @error

        @value
      end
    end

    def cancel
      @mutex.synchronize do
        return false if @cancel_requested || @state == :settled

        @cancel_requested = true
      end
      @api.cancel_function_call(@call_id).zero?
    end

    def done?
      @mutex.synchronize { @state == :settled }
    end

    # Internal: the callback delivers an envelope owning its handle references
    # and pinned host values (or an error). Waiters build and cache Ruby values.
    def complete(outcome)
      @mutex.synchronize do
        return if @state == :settled

        @outcome = outcome
        @state = :settled
        @condition.broadcast
      end
    end
  end
end
