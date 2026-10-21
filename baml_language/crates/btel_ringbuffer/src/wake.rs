use crate::sync::{AtomicUsize, Mutex, Ordering, Padded, thread};

const NOTIFIED: usize = 1;
const ARMED: usize = 2;

pub(crate) struct Wake {
    state: Padded<AtomicUsize>,
    target: Mutex<Option<thread::Thread>>,
}

impl Wake {
    pub(crate) fn new() -> Self {
        Self {
            state: Padded(AtomicUsize::new(NOTIFIED)),
            target: Mutex::new(None),
        }
    }

    pub(crate) fn bind(&self) {
        *self.target.lock().unwrap() = Some(thread::current());
    }

    #[inline(always)]
    pub(crate) fn notify(&self) {
        // Always an RMW: even an already-notified producer must release its
        // publication to the consumer's next arm/recheck. A relaxed fast-path
        // load followed by skipping this RMW can lose a wake on weak hardware.
        if self.state.0.fetch_or(NOTIFIED, Ordering::Release) == ARMED {
            self.unpark();
        }
    }

    #[cold]
    #[inline(never)]
    fn unpark(&self) {
        if let Some(target) = self.target.lock().unwrap().as_ref() {
            target.unpark();
        }
    }

    pub(crate) fn arm(&self) {
        // Acquires all preceding notifications before the caller rechecks ALL
        // its rings. A later notifier observes ARMED and deposits a park token.
        self.state.0.swap(ARMED, Ordering::AcqRel);
    }

    pub(crate) fn disarm(&self) {
        self.state.0.swap(NOTIFIED, Ordering::AcqRel);
    }

    pub(crate) fn park() {
        thread::park();
    }
}
