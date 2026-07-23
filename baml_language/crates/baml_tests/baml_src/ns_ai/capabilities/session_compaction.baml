class CompactionPolicy { name: string? }
class CompactionResult { changed: bool }
interface CompactableSession requires Session {
  function compact(self, policy: CompactionPolicy) -> CompactionResult throws baml.errors.UnknownError
}
