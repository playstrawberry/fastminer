use std::collections::{HashSet, VecDeque};

pub type Pos = [i32; 3];

pub trait Host {
    fn same_kind(&mut self, pos: Pos) -> bool;
    /// Whether the block was broken. Anything else ends the whole chain: it is never retried,
    /// and nothing is assumed about the block, which another plugin may have changed itself.
    fn break_one(&mut self, pos: Pos) -> bool;
}

pub struct Budget {
    pub probes: usize,
    pub breaks: usize,
}

pub struct Walk {
    pub broken: usize,
    pub limit: usize,
    pub pending: VecDeque<Pos>,
    pub seen: HashSet<Pos>,
}

impl Walk {
    pub fn new(root: Pos, limit: usize) -> Self {
        let mut walk = Self {
            broken: 1,
            limit: limit.clamp(1, 256),
            pending: VecDeque::new(),
            seen: HashSet::from([root]),
        };
        walk.expand(root);
        walk
    }

    pub fn done(&self) -> bool {
        self.pending.is_empty() || self.broken >= self.limit
    }

    pub fn advance(&mut self, host: &mut impl Host, budget: &mut Budget) {
        if self.done() || budget.probes == 0 || budget.breaks == 0 {
            return;
        }
        let Some(pos) = self.pending.pop_front() else {
            return;
        };
        budget.probes -= 1;
        if !host.same_kind(pos) {
            return;
        }
        budget.breaks -= 1;
        if host.break_one(pos) {
            self.broken += 1;
            self.expand(pos);
        } else {
            self.pending.clear();
        }
    }

    fn expand(&mut self, pos: Pos) {
        if self.broken >= self.limit {
            return;
        }
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if [x, y, z] == [0, 0, 0] {
                        continue;
                    }
                    let (Some(x), Some(y), Some(z)) = (
                        pos[0].checked_add(x),
                        pos[1].checked_add(y),
                        pos[2].checked_add(z),
                    ) else {
                        continue;
                    };
                    let next = [x, y, z];
                    if self.seen.len() < 1 + 26 * self.limit && self.seen.insert(next) {
                        self.pending.push_back(next);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Mock {
        blocks: HashSet<Pos>,
        denied: HashSet<Pos>,
        calls: Vec<Pos>,
        reads: usize,
        stop_after: Option<usize>,
    }

    impl Host for Mock {
        fn same_kind(&mut self, p: Pos) -> bool {
            self.reads += 1;
            self.blocks.contains(&p)
        }
        fn break_one(&mut self, p: Pos) -> bool {
            self.calls.push(p);
            if self.stop_after.is_some_and(|n| self.calls.len() > n) || self.denied.contains(&p) {
                return false;
            }
            assert!(
                self.blocks.remove(&p),
                "never break a stale/nonmatching block"
            );
            true
        }
    }

    fn run(walk: &mut Walk, host: &mut Mock, probes: usize, breaks: usize) -> usize {
        let mut ticks = 0;
        while !walk.done() {
            ticks += 1;
            assert!(ticks < 20000);
            let mut b = Budget { probes, breaks };
            let (reads0, breaks0) = (host.reads, host.calls.len());
            for _ in 0..probes {
                if walk.done() || b.probes == 0 || b.breaks == 0 {
                    break;
                }
                walk.advance(host, &mut b);
            }
            assert!(host.reads - reads0 <= probes);
            assert!(host.calls.len() - breaks0 <= breaks);
            assert!(walk.seen.len() <= 1 + 26 * walk.limit);
        }
        ticks
    }

    #[test]
    fn sixty_four_blocks_are_spread_over_ticks_without_duplicates() {
        let mut host = Mock {
            blocks: (1..64).map(|y| [0, y, 0]).collect(),
            ..Mock::default()
        };
        let mut walk = Walk::new([0, 0, 0], 64);
        let ticks = run(&mut walk, &mut host, 104, 4);
        assert_eq!(walk.broken, 64);
        assert_eq!(host.calls.len(), 63);
        assert!(ticks >= 16);
        assert!(host.blocks.is_empty());
        assert!(host.calls.iter().collect::<HashSet<_>>().len() == 63);
    }

    #[test]
    fn per_break_limit_includes_original_block_and_diagonals_connect() {
        let mut host = Mock {
            blocks: (1..100).map(|n| [n, n, n]).collect(),
            ..Mock::default()
        };
        let mut walk = Walk::new([0, 0, 0], 8);
        run(&mut walk, &mut host, 52, 2);
        assert_eq!(walk.broken, 8);
        assert_eq!(host.calls.len(), 7);
        assert_eq!(host.blocks.len(), 92);
    }

    #[test]
    fn denied_block_is_not_a_bridge_and_is_not_retried() {
        let mut host = Mock {
            blocks: HashSet::from([[1, 0, 0], [2, 0, 0], [3, 0, 0]]),
            denied: HashSet::from([[1, 0, 0]]),
            ..Mock::default()
        };
        let mut walk = Walk::new([0, 0, 0], 64);
        run(&mut walk, &mut host, 1, 1);
        assert_eq!(walk.broken, 1);
        assert_eq!(host.calls, [[1, 0, 0]]);
        assert!(host.blocks.contains(&[3, 0, 0]));
    }

    #[test]
    fn one_refused_break_ends_every_other_branch() {
        // [-1, 0, 0] is probed first; the opposite branch must never be touched afterwards.
        let mut host = Mock {
            blocks: HashSet::from([[-1, 0, 0], [1, 0, 0], [2, 0, 0]]),
            denied: HashSet::from([[-1, 0, 0]]),
            ..Mock::default()
        };
        let mut walk = Walk::new([0, 0, 0], 64);
        run(&mut walk, &mut host, 208, 8);
        assert!(walk.done() && walk.pending.is_empty());
        assert_eq!(walk.broken, 1);
        assert_eq!(host.calls, [[-1, 0, 0]]);
        assert!(host.blocks.contains(&[1, 0, 0]) && host.blocks.contains(&[2, 0, 0]));
    }

    #[test]
    fn stop_drops_all_pending_work_and_air_is_never_broken() {
        let mut host = Mock {
            blocks: (1..64).map(|y| [0, y, 0]).collect(),
            stop_after: Some(2),
            ..Mock::default()
        };
        let mut walk = Walk::new([0, 0, 0], 64);
        run(&mut walk, &mut host, 208, 8);
        assert_eq!(walk.broken, 3);
        assert!(walk.pending.is_empty());
    }

    #[test]
    fn dense_vein_memory_and_overflow_are_bounded() {
        let mut host = Mock {
            blocks: (-8..8)
                .flat_map(|x| (-8..8).flat_map(move |y| (-8..8).map(move |z| [x, y, z])))
                .collect(),
            ..Mock::default()
        };
        host.blocks.remove(&[0, 0, 0]);
        let mut walk = Walk::new([0, 0, 0], 256);
        run(&mut walk, &mut host, 208, 8);
        assert_eq!(walk.broken, 256);
        let mut edge = Walk::new([i32::MAX, i32::MIN, i32::MAX], 64);
        run(&mut edge, &mut Mock::default(), 1, 1);
        let unique: HashMap<_, _> = host.calls.iter().map(|p| (*p, 1)).collect();
        assert_eq!(unique.len(), host.calls.len());
    }
}
