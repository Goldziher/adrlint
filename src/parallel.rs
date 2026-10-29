//! A small deterministic parallel map over the standard library's scoped threads.
//!
//! Work is handed out one item at a time from a shared counter, so a few large ADRs do not leave
//! other workers idle, and results are returned in input order, so output never depends on thread
//! timing.

use std::io;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Stack size of a worker thread: parsing a deeply nested document needs more than a platform's
/// default thread stack (2 MiB for spawned threads, 1 MiB for the Windows main thread).
const WORKER_STACK_BYTES: usize = 16 << 20;

/// Applies `function` to every item on up to one thread per core and returns the results in input
/// order.
///
/// Every item runs on a worker thread with [`WORKER_STACK_BYTES`] of stack, even a single item on a
/// single core, so the stack a document gets never depends on the input size or the machine. A
/// worker that cannot be spawned is an error; a worker that panics re-raises its panic here.
///
/// # Errors
///
/// Returns the operating system error when a worker thread cannot be spawned.
pub fn map<T, R, F>(items: &[T], function: F) -> io::Result<Vec<R>>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let threads = thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(items.len());

    let next = AtomicUsize::new(0);
    let next = &next;
    let function = &function;
    let mut indexed = thread::scope(|scope| -> io::Result<Vec<(usize, R)>> {
        let mut workers = Vec::with_capacity(threads);
        for _ in 0..threads {
            let worker = thread::Builder::new()
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    let mut local = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        local.push((index, function(item)));
                    }
                    local
                });
            match worker {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    // The workers already running drain the queue; nothing new is handed out.
                    next.store(items.len(), Ordering::Relaxed);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        let mut indexed = Vec::with_capacity(items.len());
        for worker in workers {
            match worker.join() {
                Ok(local) => indexed.extend(local),
                Err(payload) => panic::resume_unwind(payload),
            }
        }
        Ok(indexed)
    })?;
    indexed.sort_by_key(|(index, _)| *index);
    Ok(indexed.into_iter().map(|(_, result)| result).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_results_in_input_order() {
        let items = (0..500).collect::<Vec<usize>>();
        assert_eq!(
            map(&items, |item| item * 2).unwrap(),
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn should_run_a_single_item_on_a_worker_thread() {
        let caller = thread::current().id();
        let ran = map(&[()], |()| thread::current().id()).unwrap();
        assert_ne!(ran, [caller], "a single item must still get the worker stack");
    }

    #[test]
    fn should_handle_empty_and_single_inputs() {
        assert!(map(&Vec::<u8>::new(), |item| *item).unwrap().is_empty());
        assert_eq!(map(&[7_u8], |item| *item).unwrap(), [7]);
    }
}
