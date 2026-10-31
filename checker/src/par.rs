//! A tiny batch helper: map a function over a slice on up to `threads` scoped threads,
//! keeping the results in input order. The checker takes its thread count as a flag; with 1
//! it runs on the calling thread.

/// `items.map(f)`, in order, on at most `threads` threads (contiguous chunks).
pub fn par_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let threads = threads.clamp(1, items.len().max(1));
    if threads == 1 {
        return items.iter().map(&f).collect();
    }
    let per = items.len().div_ceil(threads);
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(per)
            .map(|chunk| s.spawn(move || chunk.iter().map(f).collect::<Vec<R>>()))
            .collect();
        let mut out = Vec::with_capacity(items.len());
        for h in handles {
            match h.join() {
                Ok(v) => out.extend(v),
                Err(p) => std::panic::resume_unwind(p),
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_keep_input_order_at_any_thread_count() {
        let v: Vec<u32> = (0..103).collect();
        let one = par_map(&v, 1, |x| x * 2);
        for t in [2, 3, 8, 500] {
            assert_eq!(par_map(&v, t, |x| x * 2), one);
        }
        assert!(par_map(&Vec::<u32>::new(), 4, |x| *x).is_empty());
    }
}
