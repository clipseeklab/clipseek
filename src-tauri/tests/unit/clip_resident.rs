use super::*;
use crate::error::AppError;
use std::sync::atomic::AtomicUsize;

struct Model(Arc<AtomicUsize>);
impl Drop for Model {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn idle_model_is_dropped_once_and_foreground_reload_works() {
    let cache = ModelCache::default();
    let drops = Arc::new(AtomicUsize::new(0));
    let model = cache.get_or_try_init(|| Ok(Model(drops.clone()))).unwrap();
    let old = Arc::downgrade(&model);
    drop(model);
    let now = Instant::now();
    let timeout = Duration::from_secs(300);
    assert!(!cache.release_if_idle(now, timeout));
    assert!(!cache.release_if_idle(now + timeout - Duration::from_nanos(1), timeout));
    assert!(cache.release_if_idle(now + timeout, timeout));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(old.upgrade().is_none());
    assert!(!cache.release_if_idle(now + timeout, timeout));
    let reloaded = cache.get_or_try_init(|| Ok(Model(drops.clone()))).unwrap();
    assert_eq!(Arc::strong_count(&reloaded), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn active_work_and_even_a_brief_acquisition_restart_the_idle_clock() {
    let cache = ModelCache::default();
    let model = cache.get_or_try_init(|| Ok(7)).unwrap();
    let now = Instant::now();
    let timeout = Duration::from_secs(300);
    assert!(!cache.release_if_idle(now, timeout));
    assert!(!cache.release_if_idle(now + timeout * 10, timeout));
    drop(model);
    assert!(!cache.release_if_idle(now + timeout * 10, timeout));
    drop(
        cache
            .get_or_try_init(|| panic!("must share cached model"))
            .unwrap(),
    );
    assert!(!cache.release_if_idle(now + timeout * 11, timeout));
    assert!(cache.release_if_idle(now + timeout * 12, timeout));
}

#[test]
fn readiness_does_not_initialize_or_keep_an_idle_model_alive() {
    let cache = ModelCache::default();
    assert_eq!(cache.inspect(|_: &i32| true), None);
    drop(cache.get_or_try_init(|| Ok(42)).unwrap());
    let now = Instant::now();
    let timeout = Duration::from_secs(300);
    assert!(!cache.release_if_idle(now, timeout));
    assert_eq!(cache.inspect(|value| *value), Some(42));
    assert!(cache.release_if_idle(now + timeout, timeout));
    assert_eq!(cache.inspect(|value| *value), None);
}

#[test]
fn eviction_blocks_background_reload_even_after_foreground_use() {
    let cache = ModelCache::default();
    drop(cache.get_for_warmup(|| Ok(1)).unwrap().unwrap());
    assert!(cache.release_if_idle(Instant::now(), Duration::ZERO));
    assert!(cache.was_evicted());
    assert!(cache
        .get_for_warmup(|| panic!("must not rewarm after eviction"))
        .unwrap()
        .is_none());
    drop(cache.get_or_try_init(|| Ok(2)).unwrap());
    assert!(cache
        .get_for_warmup(|| panic!("must not touch reloaded model"))
        .unwrap()
        .is_none());
    assert_eq!(cache.inspect(|value| *value), Some(2));
}

#[test]
fn failed_initialization_can_retry_without_publishing_a_model() {
    let cache = ModelCache::default();
    assert!(cache
        .get_or_try_init(|| Err::<i32, _>(AppError::Msg("failed load".into())))
        .is_err());
    assert_eq!(cache.inspect(|value| *value), None);
    assert!(!cache.was_evicted());
    assert_eq!(*cache.get_or_try_init(|| Ok(9)).unwrap(), 9);
}

#[test]
fn concurrent_initialization_publishes_only_one_model_and_cleanup_cannot_interrupt_it() {
    let cache = Arc::new(ModelCache::default());
    let loads = Arc::new(AtomicUsize::new(0));
    let (entered, waiting) = std::sync::mpsc::channel();
    let (resume, blocked) = std::sync::mpsc::channel();
    let loader = cache.clone();
    let counter = loads.clone();
    let first = std::thread::spawn(move || {
        loader
            .get_or_try_init(|| {
                counter.fetch_add(1, Ordering::SeqCst);
                entered.send(()).unwrap();
                blocked.recv().unwrap();
                Ok(5)
            })
            .unwrap()
    });
    waiting.recv().unwrap();
    assert!(!cache.release_if_idle(Instant::now(), Duration::ZERO));
    assert_eq!(cache.inspect(|value| *value), None);
    let another = cache.clone();
    let second = std::thread::spawn(move || {
        another
            .get_or_try_init(|| panic!("duplicate load"))
            .unwrap()
    });
    resume.send(()).unwrap();
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert!(!cache.release_if_idle(Instant::now(), Duration::ZERO));
    drop((first, second));
    assert!(cache.release_if_idle(Instant::now(), Duration::ZERO));
}
