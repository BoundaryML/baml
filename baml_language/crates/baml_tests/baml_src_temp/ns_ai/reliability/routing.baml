class Route {
  name: string,
  provider: Provider,
  reason: string,
}

interface Router {
  function choose(self, task_name: string, tags: map<string, string>) -> Route throws never
}

function route<T>(task: Task<T>, router: Router) -> Task<T> throws never {
  let selected = router.choose(task.identity?.name ?? "anonymous", task.tags);
  task.with_provider(selected.provider)
}

