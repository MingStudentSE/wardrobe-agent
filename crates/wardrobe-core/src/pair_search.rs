//! Exact lazy search for role-slot combinations with non-monotone pair rewards.
//! Independent of persistence: callers supply real eligible clothing identifiers.
use std::collections::{BTreeMap, BinaryHeap, HashMap};

pub(crate) type PairWeights = BTreeMap<(String, String), i64>;

fn edge(weights: &PairWeights, a: &str, b: &str) -> i64 {
    let key = if a < b { (a, b) } else { (b, a) };
    weights
        .get(&(key.0.to_owned(), key.1.to_owned()))
        .copied()
        .unwrap_or(0)
}

/// Emits complete combinations in descending (pair reward bound, negative rank
/// sum). Bounds are eligible-aware: every unresolved slot pair contributes the
/// greatest nonnegative learned edge actually present between its member IDs —
/// zero when none — precomputed by one sparse scan over the learned weights via
/// ID-to-slot membership, never a candidate Cartesian product. A partial state
/// therefore never expands on role pairs without eligible learned edges, yet
/// its bound stays an admissible over-estimate: resolved edges count exactly,
/// unresolved negative edges can only lower true totals, and empty placeholder
/// IDs carry no edge. Unlike neighbor-frontier search, it does not assume
/// monotone pair rewards.
pub(crate) struct PairSearch<'a> {
    slots: &'a [Vec<(String, usize)>],
    fixed: &'a [String],
    weights: &'a PairWeights,
    heap: BinaryHeap<(i64, i64, Vec<usize>)>,
    slot_pair_bound: Vec<Vec<i64>>,
}

impl<'a> PairSearch<'a> {
    pub(crate) fn new(
        slots: &'a [Vec<(String, usize)>],
        fixed: &'a [String],
        weights: &'a PairWeights,
    ) -> Self {
        let mut slot_of: HashMap<&str, usize> = HashMap::new();
        for (slot, members) in slots.iter().enumerate() {
            for (id, _) in members {
                slot_of.insert(id.as_str(), slot);
            }
        }
        let mut slot_pair_bound = vec![vec![0i64; slots.len()]; slots.len()];
        for ((a, b), weight) in weights {
            let weight = *weight;
            if let (Some(&i), Some(&j)) = (slot_of.get(a.as_str()), slot_of.get(b.as_str())) {
                if i != j && weight > slot_pair_bound[i][j] {
                    slot_pair_bound[i][j] = weight;
                    slot_pair_bound[j][i] = weight;
                }
            }
        }
        let mut search = Self {
            slots,
            fixed,
            weights,
            heap: BinaryHeap::new(),
            slot_pair_bound,
        };
        if slots.iter().all(|s| !s.is_empty()) {
            let (pair, rank) = search.bound(&[]);
            search.heap.push((pair, rank, vec![]));
        }
        search
    }

    fn bound(&self, indices: &[usize]) -> (i64, i64) {
        let mut selected: Vec<&str> = self.fixed.iter().map(String::as_str).collect();
        let mut rank = 0i64;
        for (slot, index) in self.slots.iter().zip(indices) {
            selected.push(&slot[*index].0);
            rank -= slot[*index].1 as i64;
        }
        let mut reward = 0i64;
        for (i, a) in selected.iter().enumerate() {
            for b in &selected[i + 1..] {
                reward += edge(self.weights, a, b);
            }
        }
        let remaining = &self.slots[indices.len()..];
        for slot in remaining {
            rank -= slot.iter().map(|(_, r)| *r).min().unwrap_or(0) as i64;
        }
        // Selected IDs keep exact eligible maxima toward every unresolved slot.
        for a in &selected {
            for slot in remaining {
                reward += slot
                    .iter()
                    .map(|(b, _)| edge(self.weights, a, b))
                    .max()
                    .unwrap_or(0)
                    .max(0);
            }
        }
        // Unresolved slot pairs contribute their precomputed sparse bounds.
        for i in indices.len()..self.slots.len() {
            for j in i + 1..self.slots.len() {
                reward += self.slot_pair_bound[i][j];
            }
        }
        (reward, rank)
    }
}

impl Iterator for PairSearch<'_> {
    type Item = Vec<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some((_, _, indices)) = self.heap.pop() {
            if indices.len() == self.slots.len() {
                return Some(indices);
            }
            for i in 0..self.slots[indices.len()].len() {
                let mut child = indices.clone();
                child.push(i);
                let (reward, rank) = self.bound(&child);
                self.heap.push((reward, rank, child));
            }
        }
        None
    }
}
