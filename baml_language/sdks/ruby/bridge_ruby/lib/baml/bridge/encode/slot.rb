# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Wire = ::BamlBridge::Cffi::V1

      class Refusal < TypeError; end
      class Fault < Refusal; end

      module Slot
        module_function

        def union(members)
          members = flatten(members)
          distinct = members.uniq { |item| canonical(item) }
          distinct.length == 1 ? distinct.first : [:union, distinct]
        end

        def flatten(members)
          members.flat_map { |item| item[0] == :union ? flatten(item[1]) : [item] }
        end

        # Aliases are not supported by the foundation. Leave their heads intact
        # rather than following an unguarded cycle; later ports add unfolding here.
        def unfold(slot)
          slot
        end

        def stated_form(slot)
          case slot[0]
          when :union then union(slot[1].map { |item| stated_form(item) })
          when :list then [:list, stated_form(slot[1])]
          when :map then [:map, stated_form(slot[1]), stated_form(slot[2])]
          when :class then [:class, slot[1], (slot[2] || []).map { |item| stated_form(item) }]
          else slot
          end
        end

        # Hash keys use one spelling for every type, including unions nested
        # inside function signatures. Keep the first-seen spelling on the wire.
        def canonical(slot)
          case slot[0]
          when :union
            members = flatten(slot[1]).map { |item| canonical(item) }.uniq.sort_by { |item| Marshal.dump(item) }
            members.length == 1 ? members.first : [:union, members]
          when :literal then [:literal, slot.length == 3 ? slot[1] : Rules.kind(slot.last), slot.last]
          when :list then [:list, canonical(slot[1])]
          when :map then [:map, canonical(slot[1]), canonical(slot[2])]
          when :class then [:class, slot[1], (slot[2] || []).map { |item| canonical(item) }]
          when :function
            [:function, slot[1].map { |name, ty, optional| [name, canonical(ty), optional] }, canonical(slot[2]), canonical(slot[3])]
          else slot
          end
        end

        def equivalent?(left, right)
          canonical(left) == canonical(right)
        end

        def substitute(slot, args)
          case slot[0]
          when :param then args.fetch(slot[1]) { [:other] }
          when :list then [:list, substitute(slot[1], args)]
          when :map then [:map, substitute(slot[1], args), substitute(slot[2], args)]
          when :union then union(slot[1].map { |item| substitute(item, args) })
          when :class then [:class, slot[1], (slot[2] || []).map { |item| substitute(item, args) }]
          else slot
          end
        end

        def to_wire(slot)
          slot = stated_form(slot)
          kind, *args = slot
          fields = case kind
                   when :int, :float, :string, :bool, :null
                     { primitive: Wire::BamlTyPrimitive.new(kind: "BAML_TY_PRIMITIVE_#{kind.upcase}".to_sym) }
                   when :unknown then { unknown: Wire::BamlTyUnknown.new }
                   when :never then { never: Wire::BamlTyNever.new }
                   when :class
                     { class_ty: Wire::BamlTyClass.new(name: args[0], type_args: (args[1] || []).map { |item| to_wire(item) }) }
                   when :enum then { enum: Wire::BamlTyEnum.new(name: args[0]) }
                   when :interface then { interface: Wire::BamlTyInterface.new(name: args[0]) }
                   when :list then { list: Wire::BamlTyList.new(item: to_wire(args[0])) }
                   when :map then { map: Wire::BamlTyMap.new(key: to_wire(args[0]), value: to_wire(args[1])) }
                   when :union then { union: Wire::BamlTyUnion.new(options: args[0].map { |item| to_wire(item) }) }
                   when :literal
                     literal_kind = Rules.kind(args[0])
                     unless %i[int string bool].include?(literal_kind)
                       raise Fault, "cannot encode this literal type"
                     end
                     { literal: Wire::BamlTyLiteral.new(**{ "#{literal_kind}_value".to_sym => args[0] }) }
                   else raise Fault, "cannot encode type #{slot.inspect}"
                   end
          Wire::BamlTy.new(**fields)
        end

        def from_wire(ty)
          return [:other] unless ty

          body = ty.public_send(ty.ty) if ty.ty
          if body.respond_to?(:head) && body.head != :name
            return [:other]
          end
          case ty.ty
          when :primitive
            kind = body.kind.to_s.delete_prefix("BAML_TY_PRIMITIVE_").downcase.to_sym
            %i[int float string bool null].include?(kind) ? [kind] : [:other]
          when :unknown then [:unknown]
          when :never then [:never]
          when :class_ty then [:class, body.name, body.type_args.map { |item| from_wire(item) }]
          when :enum then [:enum, body.name]
          when :interface
            body.type_args.empty? && body.bindings.empty? ? [:interface, body.name] : [:other]
          when :list then [:list, from_wire(body.item)]
          when :map then [:map, from_wire(body.key), from_wire(body.value)]
          when :optional then union([from_wire(body.inner), [:null]])
          when :union then union(body.options.map { |item| from_wire(item) })
          when :literal
            %i[int_value string_value bool_value].include?(body.literal) ? [:literal, body.public_send(body.literal)] : [:other]
          else [:other]
          end
        end

        # Protobuf reflection includes every nested message, including type
        # annotations. Use the schema itself for depth and live-reference walks.
        def each_message(message)
          message.class.descriptor.each do |field|
            next unless field.type == :message

            value = field.get(message)
            if field.label == :repeated
              value.each { |child| yield child }
            elsif value
              yield value
            end
          end
        end

        def wire_depth(message)
          deepest = 0
          each_message(message) { |child| deepest = [deepest, 1 + wire_depth(child)].max }
          deepest
        end
      end
    end
  end
end
