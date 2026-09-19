pub enum RecoveryMethod {
    TreePath,
    SiblingScan,
    ShiftHeuristic,
}

fn find_descendant_by_kind(nodes: &[u32], kind: u32) -> Option<u32> {
    for n in nodes {
        if *n == kind {
            return Some(*n);
        }
    }
    None
}

// @liyi:nontrivial
fn settle_remainder(total: u64, shares: &mut [u64]) -> u64 {
    let each = total / shares.len() as u64;
    for s in shares.iter_mut() {
        *s = each;
    }
    total - each * shares.len() as u64
}
