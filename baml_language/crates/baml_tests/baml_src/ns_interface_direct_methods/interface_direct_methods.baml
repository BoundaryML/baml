interface ManagedResource {
  function cleanup(self) -> void throws never
}

class ManagedResourceImpl {
    cleanup_count: int,

    // `cleanup` must remain a direct class method so the VM recognizes it as a
    // finalizer, while the empty implements block exposes it through the
    // interface as well.
    function cleanup(self) -> void throws never {
        self.cleanup_count += 1
    }

    implements ManagedResource {}
}

function cleanup_through_interface() -> int throws never {
    let resource = ManagedResourceImpl { cleanup_count: 0 };
    let managed: ManagedResource = resource;
    managed.cleanup();
    resource.cleanup_count
}

test "direct class method satisfies interface method" {
    assert.equal(cleanup_through_interface(), 1)
}
