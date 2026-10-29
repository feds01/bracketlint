//! Running work on each of the machine's threads.

use std::{
    num::NonZero,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

/// The number of threads to run work on, which is the number that the machine
/// can run at once, or one if that isn't known.
pub fn threads() -> usize {
    thread::available_parallelism().map_or(1, NonZero::get)
}

/// Map `f` over `items` on `threads` threads, and return what it returns for
/// each item, in the order of `items`. Each thread takes the next item that no
/// thread has taken yet, so that a large one doesn't hold up the rest, and
/// keeps a state for `f` that `init` makes, e.g. an arena to parse into. With
/// one thread, the items are mapped on this one.
pub fn map_in_parallel<T: Sync, S, R: Send>(
    items: &[T],
    threads: usize,
    init: impl Fn() -> S + Sync,
    f: impl Fn(&mut S, &T) -> R + Sync,
) -> Vec<R> {
    if threads <= 1 || items.len() <= 1 {
        let mut state = init();
        return items.iter().map(|item| f(&mut state, item)).collect();
    }

    let next = AtomicUsize::new(0);
    let mut mapped: Vec<(usize, R)> = thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(items.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut state = init();
                    let mut mapped = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break mapped;
                        };
                        mapped.push((index, f(&mut state, item)));
                    }
                })
            })
            .collect();

        workers
            .into_iter()
            .flat_map(|worker| {
                worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });

    mapped.sort_unstable_by_key(|&(index, _)| index);
    mapped.into_iter().map(|(_, value)| value).collect()
}

#[cfg(test)]
mod tests {
    use super::map_in_parallel;

    #[test]
    fn mapped_in_parallel_in_the_order_of_the_items() {
        // The items take different times to map, so that the threads finish
        // them out of order.
        let items: Vec<u64> = (0..200).collect();
        let spin = |item: u64| (0..item % 7 * 10_000).fold(0u64, u64::wrapping_add);

        for threads in [1, 2, 8] {
            let mapped = map_in_parallel(
                &items,
                threads,
                || (),
                |(), &item| {
                    std::hint::black_box(spin(item));
                    item * 2
                },
            );

            let expected: Vec<_> = items.iter().map(|item| item * 2).collect();
            assert_eq!(mapped, expected, "with {threads} threads");
        }
    }
}
