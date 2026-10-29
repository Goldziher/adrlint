//! A small deterministic parallel map over the standard library's scoped threads.
//!
//! Work is handed out one item at a time from a shared counter, so a few large ADRs do not leave
//! other workers idle, and results are returned in input order, so output never depends on thread
//! timing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Applies `function` to every item on up to one thread per core and returns the results in input
/// order.
pub fn map<T, R, F>(items: &[T], function: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let threads = thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(items.len());
    if threads <= 1 {
        return items.iter().map(function).collect();
    }

    let next = AtomicUsize::new(0);
    let next = &next;
    let function = &function;
    let mut indexed = thread::scope(|scope| {
        let workers = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut local = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        local.push((index, function(item)));
                    }
                    local
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker thread panicked"))
            .collect::<Vec<_>>()
    });
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_results_in_input_order() {
        let items = (0..500).collect::<Vec<usize>>();
        assert_eq!(map(&items, |item| item * 2), items.iter().map(|item| item * 2).collect::<Vec<_>>());
    }

    #[test]
    fn should_handle_empty_and_single_inputs() {
        assert!(map(&Vec::<u8>::new(), |item| *item).is_empty());
        assert_eq!(map(&[7_u8], |item| *item), [7]);
    }
}
