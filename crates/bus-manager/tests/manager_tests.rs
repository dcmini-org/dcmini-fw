use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use bus_manager::{BusError, BusFactory, BusHandle, BusManager};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

// ---------------------------------------------------------------------------
// Mock factory
// ---------------------------------------------------------------------------

/// A simple mock bus for testing.
#[derive(Debug, PartialEq, Eq)]
struct MockBus {
    value: u32,
}

/// Resources needed to create a MockBus.
struct MockResources {
    value: u32,
    /// If set to true, the next `create` call will fail.
    fail_next: Arc<AtomicBool>,
}

/// Destructor token — just holds the value so we can recover resources.
struct MockDestructor {
    value: u32,
    fail_next: Arc<AtomicBool>,
}

/// Counters for tracking factory calls.
#[derive(Clone)]
struct MockCounters {
    create_count: Arc<AtomicUsize>,
    recover_count: Arc<AtomicUsize>,
}

impl MockCounters {
    fn new() -> Self {
        Self {
            create_count: Arc::new(AtomicUsize::new(0)),
            recover_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

// We need a way to pass counters into the factory. Since BusFactory is a trait
// with associated types (no instance), we use a thread-local for test tracking.
std::thread_local! {
    static COUNTERS: std::cell::RefCell<Option<MockCounters>> = const { std::cell::RefCell::new(None) };
}

fn set_counters(c: &MockCounters) {
    COUNTERS.with(|cell| *cell.borrow_mut() = Some(c.clone()));
}

fn inc_create() {
    COUNTERS.with(|cell| {
        if let Some(ref c) = *cell.borrow() {
            c.create_count.fetch_add(1, Ordering::SeqCst);
        }
    });
}

fn inc_recover() {
    COUNTERS.with(|cell| {
        if let Some(ref c) = *cell.borrow() {
            c.recover_count.fetch_add(1, Ordering::SeqCst);
        }
    });
}

/// The factory type for tests.
struct MockFactory;

#[derive(Debug, PartialEq)]
struct MockError;

impl BusFactory for MockFactory {
    type Bus = MockBus;
    type Resources = MockResources;
    type Destructor = MockDestructor;
    type Error = MockError;

    fn create(
        resources: Self::Resources,
    ) -> Result<(Self::Bus, Self::Destructor), (Self::Error, Self::Resources)>
    {
        inc_create();
        if resources.fail_next.load(Ordering::SeqCst) {
            // Reset the flag so the next attempt can succeed.
            resources.fail_next.store(false, Ordering::SeqCst);
            Err((MockError, resources))
        } else {
            let bus = MockBus { value: resources.value };
            let destructor = MockDestructor {
                value: resources.value,
                fail_next: resources.fail_next,
            };
            Ok((bus, destructor))
        }
    }

    fn recover(destructor: Self::Destructor) -> Self::Resources {
        inc_recover();
        MockResources {
            value: destructor.value,
            fail_next: destructor.fail_next,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_manager(
    value: u32,
    fail_next: bool,
) -> (BusManager<NoopRawMutex, MockFactory>, MockCounters, Arc<AtomicBool>) {
    let fail = Arc::new(AtomicBool::new(fail_next));
    let resources = MockResources { value, fail_next: fail.clone() };
    let counters = MockCounters::new();
    set_counters(&counters);
    (BusManager::new(resources), counters, fail)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[futures_test::test]
async fn acquire_creates_bus() {
    let (mgr, counters, _) = make_manager(42, false);

    let handle = mgr.acquire().await.unwrap();
    assert_eq!(handle.value, 42);
    assert_eq!(mgr.user_count(), 1);
    assert_eq!(counters.create_count.load(Ordering::SeqCst), 1);
}

#[futures_test::test]
async fn acquire_reuses_active_bus() {
    let (mgr, counters, _) = make_manager(42, false);

    let h1 = mgr.acquire().await.unwrap();
    let h2 = mgr.acquire().await.unwrap();

    assert_eq!(mgr.user_count(), 2);
    assert_eq!(counters.create_count.load(Ordering::SeqCst), 1);
    assert_eq!(h1.value, h2.value);
}

#[futures_test::test]
async fn drop_decrements_users() {
    let (mgr, _, _) = make_manager(42, false);

    let handle = mgr.acquire().await.unwrap();
    assert_eq!(mgr.user_count(), 1);
    drop(handle);
    assert_eq!(mgr.user_count(), 0);
}

#[futures_test::test]
async fn try_release_when_no_users() {
    let (mgr, counters, _) = make_manager(42, false);

    let handle = mgr.acquire().await.unwrap();
    drop(handle);

    let result = mgr.try_release().await;
    assert!(result.is_ok());
    assert_eq!(counters.recover_count.load(Ordering::SeqCst), 1);
    assert_eq!(mgr.is_active(), Some(false));
}

#[futures_test::test]
async fn try_release_with_active_users() {
    let (mgr, _, _) = make_manager(42, false);

    let _handle = mgr.acquire().await.unwrap();

    let result = mgr.try_release().await;
    assert_eq!(result, Err(BusError::InUse(1)));
    assert_eq!(mgr.is_active(), Some(true));
}

#[futures_test::test]
async fn try_release_idempotent_when_idle() {
    let (mgr, _, _) = make_manager(42, false);

    // Never acquired — already idle.
    let result = mgr.try_release().await;
    assert!(result.is_ok());
}

#[futures_test::test]
async fn acquire_after_release() {
    let (mgr, counters, _) = make_manager(42, false);

    // First cycle
    let handle = mgr.acquire().await.unwrap();
    assert_eq!(handle.value, 42);
    drop(handle);
    mgr.try_release().await.unwrap();

    // Second cycle
    let handle = mgr.acquire().await.unwrap();
    assert_eq!(handle.value, 42);
    drop(handle);

    assert_eq!(counters.create_count.load(Ordering::SeqCst), 2);
    assert_eq!(counters.recover_count.load(Ordering::SeqCst), 1);
}

#[futures_test::test]
async fn factory_error_preserves_resources() {
    let (mgr, counters, _fail) = make_manager(42, true);

    // First attempt should fail
    let result = mgr.acquire().await;
    assert!(matches!(result, Err(BusError::FactoryError(_))));
    assert_eq!(counters.create_count.load(Ordering::SeqCst), 1);

    // Retry should succeed (fail_next was reset by factory)
    let handle = mgr.acquire().await.unwrap();
    assert_eq!(handle.value, 42);
    assert_eq!(counters.create_count.load(Ordering::SeqCst), 2);
}

#[futures_test::test]
async fn multiple_cycles() {
    let (mgr, counters, _) = make_manager(7, false);

    for _ in 0..3 {
        let handle = mgr.acquire().await.unwrap();
        assert_eq!(handle.value, 7);
        drop(handle);
        mgr.try_release().await.unwrap();
    }

    assert_eq!(counters.create_count.load(Ordering::SeqCst), 3);
    assert_eq!(counters.recover_count.load(Ordering::SeqCst), 3);
}

#[futures_test::test]
async fn handle_deref_returns_correct_value() {
    let (mgr, _, _) = make_manager(99, false);

    let handle: BusHandle<'_, NoopRawMutex, MockFactory> =
        mgr.acquire().await.unwrap();

    // Access through Deref
    let bus: &MockBus = &*handle;
    assert_eq!(bus.value, 99);
}
