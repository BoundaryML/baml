# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      module Commit
        WIRE_LIMIT = 100
        ARGUMENT_DEPTH = 2
        module_function

        def depth(node)
          statement = %i[list map class callable].include?(node.kind) ? 1 + Slot.wire_depth(Slot.to_wire(node.ty)) : 0
          payload = case node.kind
                    when :enum, :handle, :callable then 1
                    when :media then node.value.name ? 2 : 1
                    when :list then 1 + (node.children.map { |child| 1 + depth(child) }.max || 0)
                    when :map, :class then 1 + (node.children.map { |_, child| 2 + depth(child) }.max || 0)
                    else 0
                    end
          [statement, payload].max
        end

        def check_depth!(depth, path, enclosing: ARGUMENT_DEPTH)
          raise Fault, "#{path}: nested too deeply" if enclosing + depth > WIRE_LIMIT
        end

        def media(node)
          value = node.value
          fields = { media: node.ty[1].upcase }
          fields[:mime_type] = value.mime_type if value.mime_type
          if value.url
            fields[:url] = value.url
          elsif value.name
            fields[:file_content] = Wire::BamlValueMediaFileContent.new(name: value.name, base64: value.base64)
          else
            fields[:base64] = value.base64
          end
          Wire::BamlValueMedia.new(**fields)
        end

        def commit(node, scope: nil)
          fields = case node.kind
                   when :null then {}
                   when :int, :float, :bool, :string then { "#{node.kind}_value".to_sym => node.value }
                   when :bigint then { bigint_value: node.value.to_s(16) }
                   when :bytes then { uint8array_value: node.value.b }
                   when :media then { media_value: media(node) }
                   when :enum
                     { enum_value: Wire::InboundEnumValue.new(name: node.ty[1], value: node.value) }
                   when :callable
                     key = HostValues.register(node.value, scope, node.ty)
                     { value_type: Slot.to_wire(node.ty), handle: Wire::BamlHandle.new(key: key, handle_type: :HOST_VALUE_CALLABLE) }
                   when :handle
                     raise Error, "BAML handles can only be sent inside a call" unless scope

                     key, kind = node.value.clone_for_wire(scope)
                     { handle: Wire::BamlHandle.new(key: key, handle_type: kind) }
                   when :list
                     { value_type: Slot.to_wire(node.ty), list_value: Wire::InboundListValue.new(
                       values: node.children.map { |child| commit(child, scope: scope) }
                     ) }
                   when :map, :class
                     entries = node.children.map do |key, child|
                       Wire::InboundMapEntry.new(string_key: key, value: commit(child, scope: scope))
                     end
                     payload = if node.kind == :map
                                 { map_value: Wire::InboundMapValue.new(entries: entries) }
                               else
                                 { class_value: Wire::InboundClassValue.new(fields: entries) }
                               end
                     { value_type: Slot.to_wire(node.ty), **payload }
                   else raise Error, "Undecided BAML encoding node #{node.kind}"
                   end
          Wire::InboundValue.new(**fields)
        end
      end
    end
  end
end
