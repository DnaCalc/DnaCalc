//! Work on a thread of its own, whose result is announced to the thread that asked for it.
//!
//! The applications fetch URLs this way: a GET blocks, so it runs on its own thread, and the owning
//! (UI) thread learns of the finished result by a thread message it already pumps. The package
//! carries no rule about what the work is; a caller pairs it with the rules it needs.
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};

/// Posted to the thread that made a [`Background`], each time a job of it has finished.
pub const WM_BACKGROUND: u32 = WM_APP + 2;

/// Jobs that run off the calling thread, and the results of those that have finished.
pub struct Background<T> {
    thread: u32,
    done: Arc<Mutex<Vec<T>>>,
}

impl<T> fmt::Debug for Background<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Background").finish_non_exhaustive()
    }
}

impl<T: Send + 'static> Background<T> {
    /// Jobs whose finish is announced to the calling thread.
    pub fn new() -> Self {
        Self {
            thread: unsafe { GetCurrentThreadId() },
            done: Arc::default(),
        }
    }

    /// Runs `work` on a new thread and queues its result, then posts [`WM_BACKGROUND`] to the
    /// thread that made this.
    pub fn spawn(&self, work: impl FnOnce() -> T + Send + 'static) {
        let (done, thread) = (self.done.clone(), self.thread);
        std::thread::spawn(move || {
            let result = work();
            done.lock().unwrap_or_else(|e| e.into_inner()).push(result);
            unsafe { PostThreadMessageW(thread, WM_BACKGROUND, 0, 0) };
        });
    }

    /// The finished results, oldest first.
    pub fn take(&self) -> Vec<T> {
        std::mem::take(&mut *self.done.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl<T: Send + 'static> Default for Background<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Native-only: a job's result reaches the thread that asked, in the order the jobs finish.
    #[test]
    fn results_are_queued_for_the_asking_thread() {
        let jobs = Background::<u32>::new();
        for n in 1..=3 {
            jobs.spawn(move || {
                std::thread::sleep(Duration::from_millis(40 * u64::from(n)));
                n
            });
        }
        let mut got = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while got.len() < 3 && Instant::now() < deadline {
            got.extend(jobs.take());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, [1, 2, 3]);
    }
}
