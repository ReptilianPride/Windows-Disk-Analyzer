use std::collections::HashMap;
use std::path::PathBuf;
use jwalk::WalkDir;
use std::sync::mpsc;

// ======================
// NODE
// ======================
#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub size: u64,
    pub children: Vec<usize>,
    pub is_dir: bool,
}

// ======================
// PROGRESS
// ======================
#[derive(Debug, Clone)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

// ======================
// NORMALIZE PATH
// ======================
fn norm(p: &PathBuf) -> PathBuf {
    p.components().collect()
}

// ======================
// BUILD TREE WITH PROGRESS
// ======================
pub fn build_tree_with_progress(
    root: &str,
    tx: mpsc::Sender<Progress>,
) -> Vec<Node> {

    let root = norm(&PathBuf::from(root));

    let mut nodes: Vec<Node> = Vec::new();
    let mut index: HashMap<PathBuf, usize> = HashMap::new();
    let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    index.insert(root.clone(), 0);

    nodes.push(Node {
        path: root.clone(),
        size: 0,
        children: Vec::new(),
        is_dir: true,
    });

    // ======================
    // COUNT FIRST (IMPORTANT FOR %)
    // ======================
    let mut total = 0;
    for _ in WalkDir::new(&root) {
        total += 1;
    }

    let mut done = 0;

    // ======================
    // WALK FILESYSTEM
    // ======================
    for entry in WalkDir::new(&root) {
        if let Ok(entry) = entry {
            done += 1;

            let _ = tx.send(Progress { done, total });

            let path = norm(&entry.path().to_path_buf());

            if let Ok(meta) = entry.metadata() {
                let is_dir = meta.is_dir();
                let size = if meta.is_file() { meta.len() } else { 0 };

                let id = *index.entry(path.clone()).or_insert_with(|| {
                    let new_id = nodes.len();
                    nodes.push(Node {
                        path: path.clone(),
                        size,
                        children: Vec::new(),
                        is_dir,
                    });
                    new_id
                });

                if let Some(parent) = path.parent().map(|p| norm(&p.to_path_buf())) {
                    children_map
                        .entry(parent)
                        .or_insert_with(Vec::new)
                        .push(path.clone());
                }

                let _ = id;
            }
        }
    }

    // ======================
    // BUILD LINKS
    // ======================
    for (parent, children) in children_map {
        if let Some(&pid) = index.get(&parent) {
            for child in children {
                if let Some(&cid) = index.get(&child) {
                    nodes[pid].children.push(cid);
                }
            }
        }
    }

    // ======================
    // SIZE AGGREGATION
    // ======================
    fn compute(nodes: &mut Vec<Node>, id: usize) -> u64 {
        let mut total = nodes[id].size;
        let children = nodes[id].children.clone();

        for c in children {
            total += compute(nodes, c);
        }

        nodes[id].size = total;
        total
    }

    compute(&mut nodes, 0);

    // ======================
    // SORT
    // ======================
    let sizes: Vec<u64> = nodes.iter().map(|n| n.size).collect();

    fn sort(nodes: &mut Vec<Node>, sizes: &[u64], id: usize) {
        nodes[id].children.sort_by(|a, b| {
            sizes[*b].cmp(&sizes[*a])
        });

        let children = nodes[id].children.clone();
        for c in children {
            sort(nodes, sizes, c);
        }
    }

    sort(&mut nodes, &sizes, 0);

    nodes
}