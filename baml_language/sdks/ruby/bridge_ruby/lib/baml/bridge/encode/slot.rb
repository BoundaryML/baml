# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Wire = ::BamlBridge::Cffi::V1

      class Refusal < TypeError; end
      class Fault < Refusal; end

      module Slot
        module_function

        def union(members, &work)
          members = flatten(members, &work)
          distinct = members.uniq { |item| canonical(item, &work) }
          distinct.length == 1 ? distinct.first : [:union, distinct]
        end

        def flatten(members, &work)
          members.flat_map do |item|
            work&.call(1)
            item[0] == :union ? flatten(item[1], &work) : [item]
          end
        end

        def unfold(slot, registry = Protocol, &work)
          return slot unless slot[0] == :alias

          seen = []
          while slot[0] == :alias && !seen.include?(slot[1])
            work&.call(1)
            seen << slot[1]
            definition = registry.alias_definition(slot[1])
            break unless definition

            slot = definition[0]
          end
          stated_form(slot, registry, &work)
        end

        # A repeated alias on one union path proves no membership. Containers
        # end that path: their children are judged against the alias afresh.
        def members(slots, registry, seen = [], &work)
          slots.flat_map do |slot|
            work&.call(1)
            case slot[0]
            when :alias
              definition = registry.alias_definition(slot[1])
              if seen.include?(slot[1])
                []
              elsif definition
                members([definition[0]], registry, seen + [slot[1]], &work)
              else
                [slot]
              end
            when :union then members(slot[1], registry, seen, &work)
            else [slot]
            end
          end
        end

        def stated_form(slot, registry = Protocol, seen = [], memo: {}.compare_by_identity, &work)
          work&.call(1)
          return memo[slot] if memo.key?(slot)

          state = ->(item) { stated_form(item, registry, seen, memo: memo, &work) }
          memo[slot] = case slot[0]
          when :alias
            raise Fault, "cannot encode cyclic alias #{slot[1]}" if seen.include?(slot[1])

            normalized = registry.normalized_alias(slot[1]) do |definition|
              if definition && !definition[1]
                stated_form(definition[0], registry, seen + [slot[1]], memo: memo, &work)
              else
                slot
              end
            end
            charge(normalized, &work)
            normalized
          when :union then union(slot[1].map(&state), &work)
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
        # Charge the expanded size before Ruby's native hashing and Marshal
        # walks. Count shared subtrees once, but charge each occurrence. This
        # also bounds cache hits without re-expanding a diamond-shaped alias.
        def charge(value, &work)
          return unless work

          visited = 0
          size = walk_size(value, {}.compare_by_identity) { visited += 1; work.call(1) }
          work.call(size - visited)
        end

        def walk_size(value, memo, &work)
          work.call(1)
          return memo[value] if memo.key?(value)

          memo[value] = 1 + children(value).sum { |child| walk_size(child, memo, &work) }
        end

        def canonical(slot, &work)
          charge(slot, &work)
          canonical_form(slot, {}.compare_by_identity, {})
        end

        # Preserve sharing in canonical forms too. Without this memo, expanding
        # each union's children again makes an alias DAG exponential in depth.
        def canonical_form(slot, memo, intern)
          return memo[slot] if memo.key?(slot)

          visit = ->(item) { canonical_form(item, memo, intern) }
          result = case slot[0]
          when :union
            members = flatten(slot[1]).map(&visit).uniq.sort_by { |item| Marshal.dump(item) }
            members.length == 1 ? members.first : [:union, members]
          when :literal then [:literal, slot.length == 3 ? slot[1] : Rules.kind(slot.last), slot.last]
          when :list then [:list, visit.call(slot[1])]
          when :map then [:map, visit.call(slot[1]), visit.call(slot[2])]
          when :class then [:class, slot[1], (slot[2] || []).map(&visit)]
          when :function
            [:function, slot[1].map { |name, ty, optional| [name, visit.call(ty), optional] }, visit.call(slot[2]), visit.call(slot[3])]
          else slot
          end
          # Marshal includes object references. Intern equal subtrees so sorting
          # cannot depend on whether the input happened to share its arrays.
          # Child IDs keep this table's hashing shallow, even for an alias DAG.
          key = case result[0]
                when :union then [:union, result[1].map(&:object_id)]
                when :list then [:list, result[1].object_id]
                when :map then [:map, result[1].object_id, result[2].object_id]
                when :class then [:class, result[1], result[2].map(&:object_id)]
                when :function
                  [:function, result[1].map { |name, ty, optional| [name, ty.object_id, optional] }, result[2].object_id, result[3].object_id]
                else result
                end
          memo[slot] = intern[key] ||= result
        end

        def equivalent?(left, right, &work)
          canonical(left, &work) == canonical(right, &work)
        end

        def children(slot)
          case slot[0]
          when :list then [slot[1]]
          when :map then [slot[1], slot[2]]
          when :union then slot[1]
          when :class then slot[2] || []
          when :function then slot[1].map { |param| param[1] } + [slot[2], slot[3]]
          else []
          end
        end

        def substitute(slot, args, &work)
          work&.call(1)
          case slot[0]
          when :param then args.fetch(slot[1]) { [:other] }
          when :list then [:list, substitute(slot[1], args, &work)]
          when :map then [:map, substitute(slot[1], args, &work), substitute(slot[2], args, &work)]
          when :union then union(slot[1].map { |item| substitute(item, args, &work) }, &work)
          when :class then [:class, slot[1], (slot[2] || []).map { |item| substitute(item, args, &work) }]
          else slot
          end
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
