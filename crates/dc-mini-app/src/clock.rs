use core::cell::RefCell;
use core::ops::Add;

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use portable_atomic::{AtomicBool, Ordering};

pub static CLOCK_SET: AtomicBool = AtomicBool::new(false);

pub struct Clock {
    time: Mutex<ThreadModeRawMutex, RefCell<time::PrimitiveDateTime>>,
}

impl Clock {
    pub const fn new() -> Self {
        Self { time: Mutex::new(RefCell::new(time::PrimitiveDateTime::MIN)) }
    }

    pub fn set(&self, time: time::PrimitiveDateTime) {
        self.time.lock(|f| *f.borrow_mut() = time);
        CLOCK_SET.store(true, Ordering::SeqCst);
    }

    pub fn get(&self, duration: time::Duration) -> time::PrimitiveDateTime {
        let time = self.time.lock(|f| f.borrow().clone());
        time.add(duration)
    }
}
