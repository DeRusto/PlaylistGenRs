//! Interleaving strategies (Python `interleave`, `interleave_in_blocks`, `interleave_by_season`).

/// Cycle `l` up to `n` items (Python `islice(cycle(l), n)`). `l` must be non-empty.
fn cycled<T: Clone>(l: &[T], n: usize) -> Vec<T> {
    l.iter().cycle().take(n).cloned().collect()
}

fn non_empty<T>(lists: &[Vec<T>]) -> Vec<&Vec<T>> {
    lists.iter().filter(|l| !l.is_empty()).collect()
}

/// Round-robin one item per list. With `repeat`, shorter lists cycle to the longest length.
pub fn interleave<T: Clone>(lists: &[Vec<T>], repeat: bool) -> Vec<T> {
    interleave_in_blocks(lists, 1, repeat)
}

/// Round-robin `block_size` consecutive items per list.
pub fn interleave_in_blocks<T: Clone>(lists: &[Vec<T>], block_size: usize, repeat: bool) -> Vec<T> {
    assert!(block_size > 0, "block_size must be positive");
    let lists = non_empty(lists);
    let max_len = lists.iter().map(|l| l.len()).max().unwrap_or(0);
    let lists: Vec<Vec<T>> = if repeat {
        lists.iter().map(|l| cycled(l, max_len)).collect()
    } else {
        lists.into_iter().cloned().collect()
    };
    let mut out = Vec::new();
    for i in (0..max_len).step_by(block_size) {
        for l in &lists {
            out.extend_from_slice(&l[i.min(l.len())..(i + block_size).min(l.len())]);
        }
    }
    out
}

/// One season per show per round. With `repeat`, shows with fewer seasons cycle them.
pub fn interleave_by_season<T: Clone>(show_seasons: &[Vec<Vec<T>>], repeat: bool) -> Vec<T> {
    let shows = non_empty(show_seasons);
    let max_seasons = shows.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut out = Vec::new();
    for idx in 0..max_seasons {
        for seasons in &shows {
            if repeat {
                out.extend_from_slice(&seasons[idx % seasons.len()]);
            } else if let Some(s) = seasons.get(idx) {
                out.extend_from_slice(s);
            }
        }
    }
    out
}
