# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Wire = ::BamlBridge::Cffi::V1

      class Refusal < TypeError; end
      class Fault < Refusal; end
      class UnboundTypeArgs < Refusal; end
      class MissingTypeArgs < Refusal; end
      class TypeArgConflict < Refusal; end
      class Ambiguous < Refusal; end
      class UntypedCallable < Refusal; end

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

        def unfold(slot, registry = Protocol)
          return slot unless slot[0] == :alias

          seen = []
          while slot[0] == :alias && !seen.include?(slot[1])
            seen << slot[1]
            definition = registry.alias_definition(slot[1])
            break unless definition

            slot = definition[0]
          end
          stated_form(slot, registry)
        end

        # A repeated alias on one union path proves no membership. Containers
        # end that path: their children are judged against the alias afresh.
        def members(slots, registry, seen = [])
          slots.flat_map do |slot|
            case slot[0]
            when :alias
              definition = registry.alias_definition(slot[1])
              if seen.include?(slot[1])
                []
              elsif definition
                members([stated_form(definition[0], registry)], registry, seen + [slot[1]])
              else
                [slot]
              end
            when :union then members(slot[1], registry, seen)
            else [slot]
            end
          end
        end

        def stated_form(slot, registry = Protocol, seen = [])
          state = ->(item) { stated_form(item, registry, seen) }
          case slot[0]
          when :alias
            definition = registry.alias_definition(slot[1])
            return slot unless definition && !definition[1]
            raise Fault, "cannot encode cyclic alias #{slot[1]}" if seen.include?(slot[1])

            stated_form(definition[0], registry, seen + [slot[1]])
          when :union then union(slot[1].map(&state))
          when :list then [:list, state.call(slot[1])]
          when :map then [:map, state.call(slot[1]), state.call(slot[2])]
          when :class then [:class, slot[1], (slot[2] || []).map(&state)]
          when :function
            [:function, slot[1].map { |name, ty, optional| [name, state.call(ty), optional] }, state.call(slot[2]), state.call(slot[3])]
          else slot
          end
        end

        def literal_kind(slot)
          slot.length == 3 ? slot[1] : Rules.kind(slot[1])
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

        def map_children(slot, &map)
          case slot[0]
          when :list then [:list, map.call(slot[1])]
          when :map then [:map, map.call(slot[1]), map.call(slot[2])]
          when :union then union(slot[1].map(&map))
          when :class then [:class, slot[1], (slot[2] || []).map(&map)]
          when :function
            [:function, slot[1].map { |name, ty, optional| [name, map.call(ty), optional] }, map.call(slot[2]), map.call(slot[3])]
          else slot
          end
        end

        def substitute(slot, args, indexed: true)
          return args.fetch(slot[indexed ? 1 : 2], nil) || slot if slot[0] == :param

          map_children(slot) { |child| substitute(child, args, indexed: indexed) }
        end

        def params(slot)
          return [slot] if slot[0] == :param

          found = []
          map_children(slot) { |child| found.concat(params(child)); child }
          found
        end

        def mentions_param?(slot)
          !params(slot).empty?
        end

        def open?(slot)
          return true if %i[hole opaque].include?(slot[0])

          found = false
          map_children(slot) { |child| found ||= open?(child); child }
          found
        end

        def to_wire(slot)
          slot = stated_form(slot)
          kind, *args = slot
          fields = case kind
                   when :int, :bigint, :float, :string, :bool, :null, :bytes
                     { primitive: Wire::BamlTyPrimitive.new(kind: "BAML_TY_PRIMITIVE_#{kind.upcase}".to_sym) }
                   when :media
                     { media: Wire::BamlTyMedia.new(kind: "BAML_TY_MEDIA_KIND_#{args[0].upcase}".to_sym) }
                   when :alias then { type_alias: Wire::BamlTyTypeAlias.new(name: args[0]) }
                   when :enum_variant then { enum_variant: Wire::BamlTyEnumVariant.new(name: args[0], variant: args[1]) }
                   when :unknown then { unknown: Wire::BamlTyUnknown.new }
                   when :never then { never: Wire::BamlTyNever.new }
                   when :class
                     { class_ty: Wire::BamlTyClass.new(name: args[0], type_args: (args[1] || []).map { |item| to_wire(item) }) }
                   when :enum then { enum: Wire::BamlTyEnum.new(name: args[0]) }
                   when :interface then { interface: Wire::BamlTyInterface.new(name: args[0]) }
                   when :list then { list: Wire::BamlTyList.new(item: to_wire(args[0])) }
                   when :map then { map: Wire::BamlTyMap.new(key: to_wire(args[0]), value: to_wire(args[1])) }
                   when :union then { union: Wire::BamlTyUnion.new(options: args[0].map { |item| to_wire(item) }) }
                   when :function
                     params = args[0].map do |name, ty, optional|
                       fields = { ty: to_wire(ty), mode: optional ? :BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL : :BAML_TY_FUNCTION_PARAM_MODE_REQUIRED }
                       fields[:name] = name if name
                       Wire::BamlTyFunctionParam.new(**fields)
                     end
                     { function: Wire::BamlTyFunction.new(params: params, ret: to_wire(args[1]), throws: to_wire(args[2])) }
                   when :literal
                     literal_kind = literal_kind(slot)
                     unless %i[int bigint string bool].include?(literal_kind)
                       raise Fault, "cannot encode this literal type"
                     end
                     value = literal_kind == :bigint ? slot.last.to_s(10) : slot.last
                     { literal: Wire::BamlTyLiteral.new(**{ "#{literal_kind}_value".to_sym => value }) }
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
            %i[int bigint float string bool null bytes].include?(kind) ? [kind] : [:other]
          when :unknown then [:unknown]
          when :never then [:never]
          when :function
            return [:other] unless body.generic_params.empty?

            [:function, body.params.map { |param| [param.has_name? ? param.name : nil, from_wire(param.ty),
              param.mode == :BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL] }, from_wire(body.ret), from_wire(body.throws)]
          when :class_ty then [:class, body.name, body.type_args.map { |item| from_wire(item) }]
          when :enum then [:enum, body.name]
          when :enum_variant then [:enum_variant, body.name, body.variant]
          when :type_alias then body.type_args.empty? ? [:alias, body.name] : [:other]
          when :media then [:media, body.kind.to_s.delete_prefix("BAML_TY_MEDIA_KIND_").downcase.to_sym]
          when :interface
            body.type_args.empty? && body.bindings.empty? ? [:interface, body.name] : [:other]
          when :list then [:list, from_wire(body.item)]
          when :map then [:map, from_wire(body.key), from_wire(body.value)]
          when :optional then union([from_wire(body.inner), [:null]])
          when :union then union(body.options.map { |item| from_wire(item) })
          when :literal
            if body.literal == :bigint_value
              [:literal, :bigint, Integer(body.bigint_value, 10)]
            else
              %i[int_value string_value bool_value].include?(body.literal) ? [:literal, body.public_send(body.literal)] : [:other]
            end
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
