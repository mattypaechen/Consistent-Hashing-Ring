use murmur3::murmur3_32;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::Cursor;

// HELPER FUNCTIONS
pub fn hashify(input: &str) -> u32 {
    let mut cursor = Cursor::new(&input); // Treats our buffer as a file stream of bytes
    murmur3_32(&mut cursor, 0).unwrap() // DRY: If we want to change out the hash function later
}

#[cfg(debug_assertions)] // Do not enable when compiling for production
pub fn format_binary_32(val: &u32) -> String {
    let s = format!("{:032b}", val);
    let mut result = String::with_capacity(32 + 7); // 32 bits + 7 underscores

    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 4 == 0 {
            result.push(' ');
        }
        result.push(c);
    }
    result
}

#[cfg(debug_assertions)] // Do not enable when compiling for production
pub fn generate_gt_value(value: &str) -> String {
    let gt_hash = hashify(value);
    let mut counter = 0;
    let mut c = counter.to_string();
    let mut hash = hashify(&c[..]);

    while hash < gt_hash {
        counter += 1;
        c = counter.to_string();
        hash = hashify(&c[..]);
    }
    c
}

// CUSTOM TYPES
struct ReadableHash(u32); // Tuple struct

impl fmt::Display for ReadableHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self.0.to_string(); // Use dot notation to get the tuple value
        let formatted = s
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join("_");
        write!(f, "{}", formatted)
    }
}

struct ConsistentHash {
    ring: BTreeMap<u32, String>,
    metamap: HashMap<String, Vec<u32>>,
    num_of_vnodes: u32,
}
// Trait Implementation: What custom traits the struct has
impl fmt::Display for ConsistentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> fmt::Result {
        writeln!(f, "====== Hash Ring State ====== ")?;
        for (hash, node) in &self.ring {
            writeln!(
                f,
                "Slot [0x{0:08X}] -> {1}: {2} ={3} ",
                hash,
                node,
                format_binary_32(hash),
                ReadableHash(*hash),
            )?;
        }
        Ok(())
    }
}

/// Hash ring contains 2^32 possible slots in which we can place a node the range of hashes is (0, 2^32 -1). A key is hashed and we compare the hash
/// with the hashes of the nodes on our ring. The node's hash that is >= our key's hash is where that data is stored.
/// The successor function is used to determine the responsible node: n = min {x ∈ N | x > k} where k is the hashed key and x is a member of set of node hashes N.
/// In the case where there does not exist a node such that 2^32 - 1 >= node's hash >= hash's key, the ring wraps around using modulo arithmetic.
/// The succesor node can then be found as follows: n = min {x ∈ N}
// Struct Implementation: What the struct does
// Time: O(log N) where N is number of nodes
impl ConsistentHash {
    pub fn new() -> Self {
        Self {
            ring: BTreeMap::new(),
            metamap: HashMap::new(), // List all vnodes associated with a pnode
            num_of_vnodes: 0u32,
        }
    }

    pub fn get_node(&self, key: &str) -> String {
        if self.ring.is_empty() {
            return String::from("Empty hash ring");
        }

        let hash = hashify(key);
        // println!("hash: {0} in binary {1}", hash, format_binary_32(&hash)); // use positional args
        match self.ring.range(hash..).next() {
            Some((_, node)) => node.clone(), // Get the nearest node where hash(node) >= hash(key)
            None => self.ring.values().next().unwrap().clone(), // Wrap around the ring
        }
    }

    pub fn add_node<T: Into<String>>(&mut self, node_id: T, weight: u32) {
        let node: String = node_id.into();
        // println!("{}, {}", node, hash);

        // Add virtual nodes to help mitigate hot spots
        for i in 0..weight {
            let vnode_id = format!("{}-{}", node, i);
            let vnode_hash = hashify(&vnode_id);
            self.metamap
                .entry(node.clone())
                .and_modify(|v| v.push(vnode_hash))
                .or_insert(vec![vnode_hash]);
            self.ring.insert(vnode_hash, node.clone()); // clone b/c cannot share with all vnodes
        }
        self.num_of_vnodes += weight;
    }

    pub fn remove_node<T: Into<String>>(&mut self, node_id: T) {
        let node = node_id.into();

        if let Some(vhashes) = self.metamap.get(&node) {
            for vhash in vhashes {
                self.ring.remove(vhash);
            }
            self.metamap.remove(&node);
        }
    }

    pub fn get_replication_list(&self, key: &str, mut count: usize) -> Vec<String> {
        if count > self.metamap.len() {
            count = self.metamap.len();
        }

        let mut rep_list = vec![];
        let hash = hashify(key);
        let primary_node = self.get_node(key);

        let tail = self.ring.range(hash..);
        let head = self.ring.range(..hash);
        let circular_iterator = tail.chain(head);

        for (_vnode_hash, pnode) in circular_iterator {
            if *pnode != primary_node && !rep_list.contains(pnode) {
                rep_list.push(pnode.clone());

                if rep_list.len() == count {
                    break;
                }
            }
        }

        rep_list
    }
}

fn main() {
    println!("Consistent Hashing: The Ring");
    let mut hash_ring = ConsistentHash::new();

    let servers = ["Server1", "Server2", "Server3", "Server4", "Server5"];

    for server in servers {
        hash_ring.add_node(&server.to_string(), 100u32);
    }

    for (k, v) in hash_ring.metamap.iter() {
        let mut hash_collection = Vec::new();
        for hash in v {
            let hash_hx = format!("0x{:08X}", hash);
            hash_collection.push(hash_hx);
        }
        let f = format!("K {:#?}, V: {:#?}", k, hash_collection);
        println!("KEY_VALUE: {f}");
    }

    println!("After removing Server4::::::::::");
    hash_ring.remove_node("Server4");

    println!("{}", hash_ring);

    for (k, v) in hash_ring.metamap.iter() {
        let mut hash_collection = Vec::new();
        for hash in v {
            let hash_hx = format!("0x{:08X}", hash);
            hash_collection.push(hash_hx);
        }
        let f = format!("K {:#?}, V: {:#?}", k, hash_collection);
        println!("KEY_VALUE: {f}");
    }

    let key = "hello world".to_string();
    let selected_node = hash_ring.get_node(&key);
    println!("Selected node is {}", selected_node);
}

#[cfg(test)] // Used to ensure test functions are not in binaries
mod unit_tests {
    use super::*;

    #[test]
    fn test_hash_empty_string() {
        let empty_string = "".to_string();
        assert_eq!(hashify(&empty_string), 0);
    }

    #[test]
    fn test_multibyte() {
        let rocket = hashify("🚀");
        let helicopter = hashify("🚁");
        assert_ne!(
            rocket, helicopter,
            "Different emoji should produce different hashes"
        );

        // 1. The high-level string
        let crab_string = "🦀";
        let string_hash = hashify(crab_string);

        // 2. The raw UTF-8 bytes for the crab emoji
        // You can get these in Rust using "🦀".as_bytes()
        let crab_bytes = vec![0xF0, 0x9F, 0xA6, 0x80];

        let mut cursor = std::io::Cursor::new(crab_bytes);
        let raw_byte_hash = murmur3::murmur3_32(&mut cursor, 0).unwrap();

        // 3. Assert they are identical
        assert_eq!(
            string_hash, raw_byte_hash,
            "The hash of the string '🦀' must match the hash of its UTF-8 bytes [F0, 9F, A6, 80]"
        );
    }

    #[test]
    fn test_generate_gt_value() {
        let value = "Server5";
        let gt_value = generate_gt_value(value);
        let hash = hashify(&gt_value);

        println!("gt_value {0}, Hash value {1}", gt_value, hash);
        assert!(hash > 3978740226);
    }

    #[test]
    fn test_nearest_node() {
        let mut hash_ring = ConsistentHash::new();

        let key1 = "Server1"; // 1_266_971_498
        let key2 = "Server41"; // 2_702_264_872
        let key3 = "Server kamikazi"; //584_518_197

        let servers = ["Server1", "Server2", "Server3", "Server4", "Server5"];

        for server in servers {
            hash_ring.add_node(&server.to_string(), 100u32);
        }

        let node1 = hash_ring.get_node(&key1);
        let node2 = hash_ring.get_node(&key2);
        let node3 = hash_ring.get_node(&key3);

        assert_eq!(node1, "Server2");
        assert_eq!(node2, "Server5");
        assert_eq!(node3, "Server3");
    }

    #[test]
    fn test_ring_wraparound() {
        let mut hash_ring = ConsistentHash::new();

        let servers = ["Server1", "Server2", "Server3", "Server4", "Server5"];

        for server in servers {
            hash_ring.add_node(&server.to_string(), 100u32);
        }

        println!("{}", hash_ring);
        let gt_value = generate_gt_value("Server5-0"); // virtual node of created for Server5

        let node = hash_ring.get_node(&gt_value);
        assert_eq!(node, "Server1");
    }

    #[test]
    fn test_get_replication_list() {
        let mut hash_ring = ConsistentHash::new();
        hash_ring.add_node("Node_A", 1);
        hash_ring.add_node("Node_B", 1);
        hash_ring.add_node("Node_C", 1);
        let key = "unique key";
        let replicas = hash_ring.get_replication_list(key, 2);
        for r in &replicas {
            println!("node {} in list", r);
        }
        // REQUIREMENT 1: Correct count
        assert_eq!(
            replicas.len(),
            2,
            "Should return exactly N replicas if available"
        );

        // REQUIREMENT 2: Uniqueness
        assert_ne!(
            replicas[0], replicas[1],
            "Replicas must be distinct physical nodes"
        );
    }
}

#[cfg(test)] // Used to ensure test functions are not in binaries
mod data_storm {
    use super::*;

    #[test]
    fn run_data_storm() {
        let mut hash_ring = ConsistentHash::new();
        let total_key_count = 1000u32;

        // 1. Initial State: 3 Nodes
        let initial_nodes = vec!["Node_A", "Node_B", "Node_C"];
        let init_nodes_weight = 100u32;
        for node in &initial_nodes {
            hash_ring.add_node(node.to_string(), init_nodes_weight);
        }

        // 2. Assign 1,000 keys and record their locations
        let mut key_inventory = HashMap::new();
        for i in 0..total_key_count {
            let key = format!("key_id_{}", i);
            let assigned_node = hash_ring.get_node(&key);
            key_inventory.insert(key, assigned_node);
        }

        // 3. Add a 4th node (The Storm)
        println!("\n--- Adding Node_D ---");
        let new_node_weight = 200u32;
        hash_ring.add_node("Node_D".to_string(), new_node_weight);

        // 4. Check how many keys moved
        let mut moved_count = 0;
        let mut stayed_count = 0;

        for (key, old_node) in &key_inventory {
            let new_node = hash_ring.get_node(key);
            if old_node == &new_node {
                stayed_count += 1;
            } else {
                moved_count += 1;
                // Senior Insight: In consistent hashing, a key should ONLY
                // move if its new assignment is the newly added node.
                assert_eq!(new_node, "Node_D", "Key {} moved to a wrong node!", key);
            }
        }

        println!("Results:");
        println!("Keys that stayed put: {}", stayed_count);
        println!("Keys that moved:     {}", moved_count);
        println!(
            "Churn Rate:          {}%",
            (moved_count as f32 / 1000.0) * 100.0
        );

        // 1. Collect counts per node
        let mut key_counts: HashMap<String, usize> = HashMap::new();
        for node in hash_ring.ring.values() {
            key_counts.insert(node.clone(), 0);
        }

        for key in 0..total_key_count {
            let node = hash_ring.get_node(&format!("key_{}", key));
            *key_counts.entry(node).or_insert(0) += 1;
        }

        // 2. Calculate Mean (Average)
        let n = key_counts.len() as f32; // represents count of physical nodes
        let sum: usize = key_counts.values().sum(); // number of keys
        let mean = sum as f32 / n;

        // 2b. Calculate Residual Error (Actual - Expected Value)
        let mut residual_errors = HashMap::new();
        let total_weight = init_nodes_weight * 3 + new_node_weight;

        for (pnode, actual_key_count) in &key_counts {
            let expected_weight = if pnode == "Node_D" {
                new_node_weight
            } else {
                init_nodes_weight
            };
            let pnode_error = *actual_key_count as f32
                - ((total_key_count * expected_weight) as f32 / total_weight as f32);
            residual_errors.insert(pnode, pnode_error);
        }

        // 3. Calculate Variance and Weighted Variance
        let variance: f32 = key_counts
            .values()
            .map(|&count| {
                let diff = count as f32 - mean;
                diff * diff
            })
            .sum::<f32>()
            / n;

        let weighted_var: f32 = residual_errors.values().map(|&re| re * re).sum::<f32>() / n;

        // 4. Standard Deviation and Weighted Standard Deviation
        let std_dev = variance.sqrt();
        let weighted_std_dev = weighted_var.sqrt();

        println!("\n--- Weight Distribution ---");

        for (pnode, vnode_list) in &hash_ring.metamap {
            let count = vnode_list.len();
            println!("{}: {} vnodes ", pnode, count);
        }

        println!("\n--- Balance Analysis ---");
        for (node, count) in &key_counts {
            println!("{}: {} keys", node, count);
        }

        println!("Standard Deviation: {:.2}", std_dev);
        println!("Coefficient of Variation: {:.2}%", (std_dev / mean) * 100.0); // How hard a node is working compared to its peers. > 50 is red flag
        println!("Weighted Standard Deviation: {:.2}", weighted_std_dev);
        println!(
            "Weighted Coefficient of Variation: {:.2}%",
            (weighted_std_dev / mean) * 100.0
        ) // How hard a node is working compared to its peers. > 50 is red flag
    }
}
