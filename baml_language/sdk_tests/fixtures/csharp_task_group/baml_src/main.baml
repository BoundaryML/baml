function NewTaskGroup(limit: int, name: string?) -> baml.spawn.TaskGroup {
  baml.spawn.TaskGroup.new(limit, name = name)
}

function RoundTripTaskGroup(group: baml.spawn.TaskGroup) -> baml.spawn.TaskGroup {
  group
}
