//! Consistent hashing for distributing keys across Redis shards/nodes.
//!
//! ## Why consistent hashing?
//!
//! In a distributed rate limiter, each key (e.g. `user:123`) maps to a bucket.
//! With N Redis nodes, we need to decide which node owns which key.
//!
//! - **Naive mod**: `key % N` — adding/removing a node causes most keys to remap (thundering herd).
//! - **Consistent hashing**: Keys map to a ring; each node owns a range. Adding a node only
//!   remaps ~1/N of keys. Minimal disruption.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// Virtual nodes (vnodes) per physical node for better distribution.
const VIRTUAL_NODES: u32 = 150;

/// Consistent hash ring. Thread-safe.
pub struct ConsistentHashRing<T: Clone + Eq> {
    ring: Mutex<BTreeMap<u64, T>>,
    nodes: Mutex<Vec<T>>,
}

impl<T: Clone + Eq + Hash + std::fmt::Debug> ConsistentHashRing<T> {
    pub fn new() -> Self {
        Self {
            ring: Mutex::new(BTreeMap::new()),
            nodes: Mutex::new(Vec::new()),
        }
    }

    /// Add a node to the ring. Each node gets `VIRTUAL_NODES` virtual replicas.
    pub fn add_node(&self, node: T) {
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains(&node) {
            return;
        }
        nodes.push(node.clone());
        let mut ring = self.ring.lock().unwrap();
        for i in 0..VIRTUAL_NODES {
            let hash = hash_key(&format!("{:#?}:{}", node, i));
            ring.insert(hash, node.clone());
        }
    }

    /// Remove a node.
    pub fn remove_node(&self, node: &T) {
        let mut nodes = self.nodes.lock().unwrap();
        nodes.retain(|n| n != node);
        let mut ring = self.ring.lock().unwrap();
        ring.retain(|_, v| v != node);
    }

    /// Get the node responsible for `key`.
    pub fn get_node<K: Hash>(&self, key: K) -> Option<T> {
        let ring = self.ring.lock().unwrap();
        if ring.is_empty() {
            return None;
        }
        let hash = hash_key(key);
        // First node with hash >= key hash (clockwise on ring)
        ring.range(hash..).next().map(|(_, v)| v.clone()).or_else(|| {
            ring.iter().next().map(|(_, v)| v.clone())
        })
    }
}

impl<T: Clone + Eq + Hash + std::fmt::Debug> Default for ConsistentHashRing<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn hash_key<K: Hash>(key: K) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_key_same_node() {
        let ring = ConsistentHashRing::new();
        ring.add_node("node1");
        ring.add_node("node2");
        let n1 = ring.get_node("user:123");
        let n2 = ring.get_node("user:123");
        assert_eq!(n1, n2);
    }

    #[test]
    fn keys_distributed() {
        let ring = ConsistentHashRing::new();
        ring.add_node("a");
        ring.add_node("b");
        ring.add_node("c");
        let mut counts = std::collections::HashMap::new();
        for i in 0..100 {
            let node = ring.get_node(format!("key{}", i)).unwrap();
            *counts.entry(node).or_insert(0) += 1;
        }
        assert!(counts.len() >= 2);
    }
}
