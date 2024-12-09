// Based on the implementation of erg
// https://github.com/erg-lang/erg/blob/21caf6fe7ebdf16caeca89960ffe2ba4dd3481c8//home/kbwo/go/projects/github.com/erg-lang/erg/crates/erg_common/shared.rs#L1

use std::{
    sync::{Arc, RwLock},
    thread::ThreadId,
    time::Duration,
};

const GET_TIMEOUT: Duration = Duration::from_secs(4);
const SET_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug)]
pub struct BorrowInfo {
    location: Option<&'static std::panic::Location<'static>>,
    thread_name: String,
}

impl std::fmt::Display for BorrowInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.location {
            Some(location) => write!(
                f,
                "{}:{}, thread: {}",
                location.file(),
                location.line(),
                self.thread_name
            ),
            None => write!(f, "unknown, thread: {}", self.thread_name),
        }
    }
}

impl BorrowInfo {
    pub fn new(location: Option<&'static std::panic::Location<'static>>) -> Self {
        Self {
            location,
            thread_name: std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Shared<T: ?Sized> {
    data: Arc<RwLock<T>>,
    // #[cfg(any(feature = "backtrace", feature = "debug"))]
    last_borrowed_at: Arc<RwLock<BorrowInfo>>,
    // #[cfg(any(feature = "backtrace", feature = "debug"))]
    last_mut_borrowed_at: Arc<RwLock<BorrowInfo>>,
    lock_thread_id: Arc<RwLock<Vec<ThreadId>>>,
}

impl<T: PartialEq> PartialEq for Shared<T>
where
    RwLock<T>: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}
