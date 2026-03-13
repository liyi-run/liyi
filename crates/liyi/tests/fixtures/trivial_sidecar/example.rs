fn get_count() -> usize {
    42
}

// @liyi:trivial
fn get_label() -> &'static str {
    "label"
}

// @liyi:nontrivial
fn compute_total(items: &[u32]) -> u64 {
    items.iter().map(|x| *x as u64).sum()
}
