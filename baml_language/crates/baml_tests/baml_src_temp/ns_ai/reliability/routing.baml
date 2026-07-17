class Route {
  name: string,
  provider: Provider,
  reason: string,
}

interface Router {
  function choose(self, task_name: string, tags: map<string, string>) -> Route throws never
}

function route<T>(task: Task<T>, router: Router) -> Task<T> throws never {
  //# Choose a route from task identity and tags
  let selected = router.choose(task.identity?.name ?? "anonymous", task.tags);
  //# Rebind the immutable task to the selected provider
  task.with_provider(selected.provider)
}
