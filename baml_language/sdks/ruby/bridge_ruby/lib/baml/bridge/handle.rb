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

    def clone_for_wire(scope)
      Thread.handle_interrupt(Exception => :never) do
        @mutex.synchronize do
          raise Bridge::Error, "BAML handle is closed" unless @key[0]

          key = @api.handle_clone(@key[0])
          scope.own(@api, key)
          [key, @handle_type]
        end
      end
    end
  end

  module Bridge
    class EncodingScope
      def initialize
        @handles = []
      end

      def own(api, key)
        @handles << [api, key]
      end

      def submitted!
        @handles.clear
      end

      def close
        handles = @handles.dup
        @handles.clear
        handles.each { |api, key| api.handle_release(key) }
      end
    end
  end
end
