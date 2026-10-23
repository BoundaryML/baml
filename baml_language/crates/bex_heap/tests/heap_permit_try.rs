//! The non-waiting permit paths: `try_new_permit` and `try_acquire` succeed
//! exactly when a waiting `new_permit`/`acquire` would not have waited, and
//! hand their argument back otherwise instead of suspending.

use bex_heap::HeapPermitManager;

#[tokio::test]
async fn try_paths_succeed_while_nothing_holds_the_heap() {
    let manager = HeapPermitManager::new();
    let inactive = manager
        .try_new_permit(())
        .unwrap_or_else(|()| panic!("a fresh manager registers without waiting"));
    let active = inactive
        .try_acquire()
        .unwrap_or_else(|_| panic!("a fresh manager has a permit free"));
    // A second holder can register and run alongside the first.
    let other = manager
        .try_new_permit(())
        .unwrap_or_else(|()| panic!("permits are not exclusive of one another"))
        .try_acquire()
        .unwrap_or_else(|_| panic!("permits are not exclusive of one another"));
    drop(other);
    drop(active);
}

#[tokio::test]
async fn try_paths_hand_back_their_argument_while_the_heap_is_parked() {
    let manager = HeapPermitManager::new();
    let inactive = manager.new_permit(()).await;
    let guard = manager.request_park().await;

    let inactive = match inactive.try_acquire() {
        Ok(_) => panic!("a parked heap hands out no permit"),
        Err(inactive) => inactive,
    };
    assert!(
        manager.try_new_permit(()).is_err(),
        "a parked heap registers no holder"
    );

    drop(guard);
    let _active = inactive
        .try_acquire()
        .unwrap_or_else(|_| panic!("the guard is gone, so the permit is free again"));
    assert!(manager.try_new_permit(()).is_ok());
}
