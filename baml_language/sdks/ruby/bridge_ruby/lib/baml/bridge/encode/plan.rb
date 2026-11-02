# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Node = Struct.new(:kind, :ty, :value, :children, :path, keyword_init: true)

      class Plan
        # Ruby has no memo, and a step costs about 5 µs: use less fuel than Rust.
        STEP_BUDGET = 1 << 20

        def initialize(registry, budget: STEP_BUDGET)
          @registry = registry
          @remaining = budget
        end

        def plan(tree, slot)
          spend(1, tree.path)
          slot = Slot.unfold(Slot.stated_form(slot, @registry), @registry)
          return choose(tree, slot[1]) if slot[0] == :union
          return node(tree, [:opaque]) if tree.kind == :handle
          if %i[unknown interface].include?(slot[0])
            own = natural(tree)
            return restate(own, fill_type(own.ty))
          end

          case slot[0]
          when :null
            return node(tree, [:null]) if tree.kind == :null
          when :int, :bigint, :float, :bool, :string, :bytes
            if (fit = Rules.fit(tree.value, slot[0])) && scalar?(tree)
              return node(tree, [slot[0]], kind: slot[0], value: fit[1])
            end
          when :literal
            expected = slot.last
            kind = Slot.literal_kind(slot)
            if %i[int bigint string bool].include?(kind) && scalar?(tree) &&
               (fit = Rules.fit(tree.value, kind)) && fit[1] == expected
              return node(tree, [kind], kind: kind, value: fit[1])
            end
          when :enum, :enum_variant
            if (tree.kind == :enum && tree.name == slot[1]) || tree.kind == :string
              if @registry.enum_values(slot[1]).include?(tree.value)
                refuse(tree, "variant does not match #{slot[2]}") if slot[0] == :enum_variant && tree.value != slot[2]
                return node(tree, [:enum, slot[1]], kind: :enum)
              end
              refuse(tree, "unknown variant #{tree.value.inspect} of #{slot[1]}")
            end
          when :list
            if tree.kind == :list
              return node(tree, slot, children: tree.children.map { |child| plan(child, slot[1]) })
            end
          when :map
            if tree.kind == :map
              entries = tree.children.map do |key, child|
                [map_key(key, slot[1]), plan(child, slot[2])]
              end
              return node(tree, slot, children: entries)
            end
          when :class
            return instance(tree, slot) if tree.kind == :class && tree.name == slot[1]
          end
          return node(tree, [:media, tree.name]) if tree.kind == :media && media_fits?(tree, slot)
          refuse(tree, "cannot encode #{tree.kind} as #{slot.inspect}")
        end

        private

        def spend(steps, path)
          @remaining -= steps
          raise Fault, "#{path}: encoding exceeds the planning-step budget" if @remaining.negative?
        end

        def scalar?(tree)
          %i[int bigint float bool string bytes].include?(tree.kind)
        end

        def refuse(tree, message)
          raise Refusal, "#{tree.path}: #{message}"
        end

        def node(tree, ty, kind: tree.kind, value: tree.value, children: nil)
          Node.new(kind: kind, ty: ty, value: value, children: children, path: tree.path)
        end

        def instance(tree, slot)
          args = slot[2] || []
          fields = @registry.encoding_fields(slot[1])
          children = tree.children.map do |name, child|
            [name, plan(child, Slot.substitute(fields.fetch(name)[1], args))]
          end
          node(tree, [:class, slot[1], args], children: children)
        end

        def natural(tree)
          spend(1, tree.path)
          case tree.kind
          when :null, :int, :bigint, :float, :bool, :string, :bytes
            node(tree, [tree.kind])
          when :media then node(tree, [:media, tree.name])
          when :enum
            unless @registry.enum_values(tree.name).include?(tree.value)
              refuse(tree, "unknown variant #{tree.value.inspect} of #{tree.name}")
            end
            node(tree, [:enum, tree.name])
          when :class then instance(tree, [:class, tree.name, []])
          when :handle then node(tree, [:opaque])
          when :list
            children = tree.children.map { |child| natural(child) }
            spend(children.length, tree.path)
            inner = join(children)
            node(tree, [:list, inner], children: children)
          when :map
            children = tree.children.map do |key, child|
              [map_key(key, [:string]), natural(child)]
            end
            spend(children.length, tree.path)
            values = children.map(&:last)
            inner = join(values)
            node(tree, [:map, [:string], inner], children: children.map(&:first).zip(values))
          else refuse(tree, "cannot encode #{tree.value.class}: it has no BAML type")
          end
        end

        def map_key(key, slot)
          unless %i[string bytes].include?(key.kind)
            refuse(key, "map keys must be string")
          end
          planned = plan(key, slot)
          refuse(key, "map keys must be string") unless planned.kind == :string
          planned.value
        end

        def media_fits?(tree, slot)
          kind = if slot[0] == :media
                   slot[1]
                 elsif slot[0] == :class && (slot[2] || []).empty?
                   @registry.class_crossing(slot[1]).to_s.delete_prefix("media_").to_sym
                 end
          kind == :generic || kind == tree.name
        end

        def rank(tree, slot)
          slot = Slot.unfold(slot, @registry)
          return 0 if tree.kind == :media && media_fits?(tree, slot)
          return 0 if tree.kind == :enum && %i[enum enum_variant].include?(slot[0]) && tree.name == slot[1]
          return 2 if %i[unknown interface].include?(slot[0]) || tree.kind == :handle
          return 0 if tree.kind == :null && slot[0] == :null
          return 0 if %i[class enum].include?(tree.kind) && tree.kind == slot[0] && tree.name == slot[1]
          return 1 if %i[list map].include?(tree.kind) && tree.kind == slot[0]
          return 3 if tree.kind == :string && %i[enum enum_variant].include?(slot[0])
          return unless scalar?(tree)

          kind = slot[0] == :literal ? Slot.literal_kind(slot) : slot[0]
          fit = Rules.fit(tree.value, kind)
          fit && (fit[0] == :as_is ? 0 : 3)
        end

        def choose(tree, members)
          members = Slot.union(Slot.members(members, @registry))
          return plan(tree, members) unless members[0] == :union

          ranked = members[1].filter_map do |member|
            priority = rank(tree, member)
            [priority, member] if priority
          end.group_by(&:first).sort
          failures = []
          ranked.each do |_, group|
            fits = []
            group.each do |_, member|
              begin
                candidate = plan(tree, member)
                fits << candidate unless fits.any? { |seen| Slot.equivalent?(seen.ty, candidate.ty) }
              rescue Fault
                raise
              rescue Refusal => error
                failures << error
              end
            end
            next if fits.empty?
            return fits.first if fits.length == 1

            own = begin
              natural(tree).ty
            rescue Fault
              raise
            rescue Refusal
              nil
            end
            preferred = fits.find { |candidate| own && Slot.equivalent?(candidate.ty, own) }
            return preferred if preferred

            refuse(tree, "fits more than one union member")
          end
          raise failures.first if failures.length == 1

          refuse(tree, "cannot encode #{tree.kind}: no union member fits")
        end

        # Only holes borrow a sibling's type. Closed containers keep their
        # types: [[1], ["a"]] is (int[] | string[])[], not (int | string)[][].
        def join(nodes)
          distinct = {}
          add = ->(ty) { distinct[Slot.canonical(ty)] ||= ty }
          nodes.each { |child| add.call(child.ty) unless open_type?(child.ty) }
          nodes.map! do |child|
            if open_type?(child.ty)
              target = distinct.each_value.find { |ty| fills?(child.ty, ty) }
              if target
                child = restate(child, target)
              else
                add.call(child.ty)
              end
            end
            child
          end
          distinct.empty? ? [:hole] : Slot.union(distinct.values)
        end

        def open_type?(ty)
          case ty[0]
          when :hole, :opaque then true
          when :list then open_type?(ty[1])
          when :map then open_type?(ty[1]) || open_type?(ty[2])
          when :union then ty[1].any? { |item| open_type?(item) }
          else false
          end
        end

        def fills?(own, target)
          return true if own[0] == :hole
          return false if own[0] == :opaque || own[0] != target[0]

          case own[0]
          when :list then fills?(own[1], target[1])
          when :map then fills?(own[1], target[1]) && fills?(own[2], target[2])
          else Slot.equivalent?(own, target)
          end
        end

        def fill_type(ty)
          case ty[0]
          when :hole, :opaque then [:unknown]
          when :list then [:list, fill_type(ty[1])]
          when :map then [:map, fill_type(ty[1]), fill_type(ty[2])]
          when :union
            members = ty[1].map { |item| fill_type(item) }
            members.include?([:unknown]) ? [:unknown] : Slot.union(members)
          else ty
          end
        end

        def restate(node, target)
          return node unless open_type?(node.ty)

          ty = if fills?(node.ty, target)
                 target
               elsif target[0] == :union && (member = target[1].find { |item| fills?(node.ty, item) })
                 member
               else
                 fill_type(node.ty)
               end
          children = case node.kind
                     when :list then node.children.map { |child| restate(child, ty[1]) }
                     when :map then node.children.map { |key, child| [key, restate(child, ty[2])] }
                     else return node
                     end
          Node.new(kind: node.kind, ty: ty, value: node.value, children: children, path: node.path)
        end
      end
    end
  end
end
