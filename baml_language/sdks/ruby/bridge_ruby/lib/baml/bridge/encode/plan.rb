# frozen_string_literal: true

module Baml
  module Bridge
    module Encode
      Node = Struct.new(:kind, :ty, :value, :children, :path, keyword_init: true)

      # Fields learn one binding per parameter. A known union member absorbs
      # a variable as never; empty containers defer to later concrete evidence.
      class Solution
        def initialize
          @bindings = {}
          @conflicts = {}
        end

        def learn(template, found)
          case template[0]
          when :param
            index = template[1]
            bound = @bindings[index]
            if !bound || bound == :absorbed || (Slot.open?(bound) && !Slot.open?(found))
              @bindings[index] = found
            elsif !Slot.equivalent?(bound, found) && !Slot.open?(found)
              @conflicts[index] = true
            end
          when :list
            learn(template[1], found[1]) if found[0] == :list
          when :map
            if found[0] == :map
              learn(template[1], found[1])
              learn(template[2], found[2])
            end
          when :class
            if found[0] == :class && template[1] == found[1] && template[2].length == found[2].length
              template[2].zip(found[2]).each { |arg, actual| learn(arg, actual) }
            end
          when :union
            open, fixed = template[1].partition { |member| Slot.mentions_param?(member) }
            return unless open.length == 1

            rest = (found[0] == :union ? found[1] : [found]).reject do |member|
              fixed.any? { |known| Slot.equivalent?(known, member) }
            end
            if rest.empty?
              Slot.params(open.first).each { |param| @bindings[param[1]] ||= :absorbed }
            else
              learn(open.first, Slot.union(rest))
            end
          when :function
            if found[0] == :function && template[1].length == found[1].length
              template[1].zip(found[1]).each { |param, actual| learn(param[1], actual[1]) }
              learn(template[2], found[2])
              learn(template[3], found[3])
            end
          end
        end

        def binding(index)
          bound = @bindings[index]
          bound == :absorbed ? [:never] : bound
        end

        def conflict?(index)
          @conflicts[index]
        end

        def apply(template)
          bindings = @bindings.to_h { |index, _| [index, binding(index)] }
          Slot.substitute(template, bindings)
        end
      end

      class Plan
        # Ruby has no memo, and a step costs about 5 µs: use less fuel than Rust.
        STEP_BUDGET = 1 << 20

        def initialize(registry, budget: STEP_BUDGET)
          @registry = registry
          @remaining = budget
        end

        def plan_root(tree, slot)
          planned = plan(tree, slot)
          if planned.kind != :handle && Slot.open?(planned.ty) && Slot.mentions_param?(slot)
            name = Slot.params(slot).first[2]
            raise UnboundTypeArgs, "#{tree.path}: could not infer a type for `#{name}`; pass the call's type arguments"
          end
          restate(planned, fill_type(planned.ty))
        end

        def plan(tree, slot)
          plan_normalized(tree, Slot.stated_form(slot, @registry))
        end

        private

        def plan_normalized(tree, slot)
          spend(1, tree.path)
          slot = Slot.unfold(slot, @registry)
          return choose(tree, slot[1]) if slot[0] == :union
          return node(tree, [:opaque]) if tree.kind == :handle
          if %i[unknown interface].include?(slot[0])
            own = natural(tree)
            return restate(own, fill_type(own.ty))
          end

          case slot[0]
          when :param, :hole, :opaque
            return natural(tree)
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
              inner = solve_elements(tree.children, slot[1], tree.path)
              return node(tree, [:list, inner], children: tree.children.map { |child| plan_normalized(child, inner) })
            end
          when :map
            if tree.kind == :map
              key_slot = Slot.mentions_param?(slot[1]) ? [:string] : slot[1]
              inner = solve_elements(tree.children.map(&:last), slot[2], tree.path)
              entries = tree.children.map do |key, child|
                [map_key(key, key_slot), plan_normalized(child, inner)]
              end
              return node(tree, [:map, key_slot, inner], children: entries)
            end
          when :class
            return instance(tree, slot) if tree.kind == :class && tree.name == slot[1]
          end
          return node(tree, [:media, tree.name]) if tree.kind == :media && media_fits?(tree, slot)
          refuse(tree, "cannot encode #{tree.kind} as #{slot.inspect}")
        end

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

        def solve_elements(children, template, path)
          return template unless Slot.mentions_param?(template)

          evidence = children.map { |child| plan_normalized(child, template) }
          spend(children.length, path)
          solved = Solution.new
          solved.learn(template, join(evidence))
          result = solved.apply(template)
          holes = Slot.params(result).to_h { |param| [param[1], [:hole]] }
          Slot.stated_form(Slot.substitute(result, holes), @registry)
        end

        def instance(tree, slot = nil)
          params = @registry.class_type_params(tree.name)
          fields = @registry.encoding_fields(tree.name)
          args = slot ? slot[2] : Array.new(params.length)
          unless args.length == params.length
            raise Fault, "#{tree.path}: wrong number of type arguments for #{tree.name}"
          end
          fixed = args.map { |arg| arg unless arg && (Slot.mentions_param?(arg) || Slot.open?(arg)) }
          unless tree.type_args.empty?
            own = tree.type_args.map { |arg| Slot.stated_form(arg, @registry) }
            unless own.length == params.length && own.zip(fixed).all? { |arg, known| !known || Slot.equivalent?(arg, known) }
              refuse(tree, "instance type arguments do not match #{slot.inspect}")
            end
            fixed = own
          end
          unless fixed.all?
            solved = Solution.new
            tree.children.each do |name, child|
              template = Slot.stated_form(Slot.substitute(fields.fetch(name)[1], fixed), @registry)
              solved.learn(template, plan_normalized(child, template).ty)
            end
            fixed = fixed.each_with_index.map do |known, index|
              next known if known
              if solved.conflict?(index)
                raise TypeArgConflict, "#{tree.path}: the fields of this #{tree.name} disagree about its type argument `#{params[index]}`"
              end
              found = solved.binding(index)
              unless found && !Slot.open?(found)
                raise MissingTypeArgs, "#{tree.path}: the fields of this #{tree.name} do not determine its type arguments; use Baml.bind_types(instance, ...)"
              end
              found
            end
          end
          children = tree.children.map do |name, child|
            template = Slot.stated_form(Slot.substitute(fields.fetch(name)[1], fixed), @registry)
            [name, plan_normalized(child, template)]
          end
          node(tree, [:class, tree.name, fixed], children: children)
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
          when :class then instance(tree)
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
          planned = plan_normalized(key, slot)
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
          return 2 if %i[unknown interface param hole opaque].include?(slot[0]) || tree.kind == :handle
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
          return plan_normalized(tree, members) unless members[0] == :union

          ranked = members[1].filter_map do |member|
            priority = rank(tree, member)
            [priority, member] if priority
          end.group_by(&:first).sort
          failures = []
          ranked.each do |_, group|
            fits = []
            group.each do |_, member|
              begin
                candidate = plan_normalized(tree, member)
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
          nodes.each { |child| add.call(child.ty) unless Slot.open?(child.ty) }
          nodes.map! do |child|
            if Slot.open?(child.ty)
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
          return node unless Slot.open?(node.ty)

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
