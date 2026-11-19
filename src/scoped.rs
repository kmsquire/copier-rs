//! Thread-local values that are overridden for the duration of a closure and
//! restored afterwards, even when the closure panics.

use std::cell::RefCell;
use std::thread::LocalKey;

/// Restores the previous value of a thread-local when dropped.
pub(crate) struct Restore<T: 'static> {
    key: &'static LocalKey<RefCell<T>>,
    previous: Option<T>,
}

impl<T: 'static> Drop for Restore<T> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            self.key.with(|slot| *slot.borrow_mut() = previous);
        }
    }
}

/// Set `key` to `value` until the returned guard is dropped.
pub(crate) fn set<T: 'static>(key: &'static LocalKey<RefCell<T>>, value: T) -> Restore<T> {
    let previous = key.with(|slot| slot.replace(value));
    Restore {
        key,
        previous: Some(previous),
    }
}

/// Run `f` with `key` set to `value`.
pub(crate) fn with<T: 'static, R>(
    key: &'static LocalKey<RefCell<T>>,
    value: T,
    f: impl FnOnce() -> R,
) -> R {
    let _restore = set(key, value);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static SLOT: RefCell<u8> = const { RefCell::new(0) };
    }

    #[test]
    fn restores_after_return_and_after_panic() {
        assert_eq!(with(&SLOT, 1, || SLOT.with(|slot| *slot.borrow())), 1);
        assert_eq!(SLOT.with(|slot| *slot.borrow()), 0);
        let outcome = std::panic::catch_unwind(|| with(&SLOT, 2, || panic!("boom")));
        assert!(outcome.is_err());
        assert_eq!(SLOT.with(|slot| *slot.borrow()), 0);
    }
}
