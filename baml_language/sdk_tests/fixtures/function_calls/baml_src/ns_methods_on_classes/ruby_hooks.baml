// Ruby calls these lifecycle hooks while loading generated declarations.
function singleton_method_added() -> int { 1 }

class HookNames {
  display string
  tap string
  then string
  gem string

  function method_added() -> int { 2 }
  function ancestors() -> int { 3 }
  function decorator() -> int { 4 }
}

function name(value: HookNames) -> HookNames { value }
