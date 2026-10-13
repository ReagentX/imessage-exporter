/*!
 Shared disjoint-set implementation for handle and chat row IDs.
*/

use std::collections::BTreeMap;

/// Use this structure to compute equivalence classes of row IDs.
///
/// Use [`crate::tables::handle::HandleGroups`] to group handles by person. Use
/// [`crate::tables::chat_handle::ChatToHandle::dedupe`] to group chats by
/// participant set or `chat_lookup` links.
#[derive(Debug)]
pub struct UnionFind {
    parent: BTreeMap<i32, i32>,
    rank: BTreeMap<i32, u32>,
}

impl UnionFind {
    /// Build an empty union-find structure.
    pub(crate) fn new() -> Self {
        UnionFind {
            parent: BTreeMap::new(),
            rank: BTreeMap::new(),
        }
    }

    /// Add an element, initializing it as its own set.
    pub(crate) fn make_set(&mut self, x: i32) {
        self.parent.entry(x).or_insert(x);
        self.rank.entry(x).or_insert(0);
    }

    /// Find the representative element of the set containing `x`.
    ///
    /// If `x` has not been added via [`make_set`], it is lazily initialized as its own set.
    pub(crate) fn find(&mut self, mut x: i32) -> i32 {
        self.make_set(x);
        let mut root = x;
        while self.parent[&root] != root {
            root = self.parent[&root];
        }
        while x != root {
            let next = self.parent[&x];
            self.parent.insert(x, root);
            x = next;
        }
        root
    }

    /// Union the sets containing `x` and `y` using union by rank.
    pub(crate) fn union(&mut self, x: i32, y: i32) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        let rank_x = self.rank[&rx];
        let rank_y = self.rank[&ry];
        if rank_x < rank_y {
            self.parent.insert(rx, ry);
        } else if rank_x > rank_y {
            self.parent.insert(ry, rx);
        } else {
            self.parent.insert(ry, rx);
            self.rank.insert(rx, rank_x + 1);
        }
    }
}
