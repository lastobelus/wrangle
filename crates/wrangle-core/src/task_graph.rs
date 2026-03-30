use std::collections::{HashMap, HashSet};

use crate::errors::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeColor {
    White,
    Gray,
    Black,
}

pub fn detect_cycle(tasks: &[(String, Vec<String>)]) -> Result<(), ConfigError> {
    let id_to_idx: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (id.as_str(), i))
        .collect();

    let mut colors = vec![NodeColor::White; tasks.len()];
    let mut path = Vec::new();

    for i in 0..tasks.len() {
        if colors[i] == NodeColor::White {
            dfs_detect(i, tasks, &id_to_idx, &mut colors, &mut path)?;
        }
    }

    Ok(())
}

fn dfs_detect(
    node: usize,
    tasks: &[(String, Vec<String>)],
    id_to_idx: &HashMap<&str, usize>,
    colors: &mut Vec<NodeColor>,
    path: &mut Vec<usize>,
) -> Result<(), ConfigError> {
    colors[node] = NodeColor::Gray;
    path.push(node);

    for dep in &tasks[node].1 {
        if let Some(&dep_idx) = id_to_idx.get(dep.as_str()) {
            match colors[dep_idx] {
                NodeColor::Gray => {
                    let cycle_start = path.iter().position(|&p| p == dep_idx).unwrap();
                    let cycle_path: Vec<&str> = path[cycle_start..]
                        .iter()
                        .map(|&idx| tasks[idx].0.as_str())
                        .chain(std::iter::once(tasks[dep_idx].0.as_str()))
                        .collect();
                    return Err(ConfigError::CircularDependency {
                        cycle: cycle_path.join(" -> "),
                    });
                }
                NodeColor::White => {
                    dfs_detect(dep_idx, tasks, id_to_idx, colors, path)?;
                }
                NodeColor::Black => {}
            }
        }
    }

    path.pop();
    colors[node] = NodeColor::Black;
    Ok(())
}

pub fn topological_phases(tasks: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    let id_to_idx: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (id.as_str(), i))
        .collect();

    let mut in_degree: Vec<usize> = tasks.iter().map(|(_, deps)| deps.len()).collect();
    let mut reverse_adj: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];

    for (i, (_, deps)) in tasks.iter().enumerate() {
        for dep in deps {
            if let Some(&dep_idx) = id_to_idx.get(dep.as_str()) {
                reverse_adj[dep_idx].push(i);
            }
        }
    }

    let mut phases = Vec::new();
    let mut completed: HashSet<usize> = HashSet::new();

    loop {
        let ready: Vec<usize> = (0..tasks.len())
            .filter(|&i| !completed.contains(&i) && in_degree[i] == 0)
            .collect();

        if ready.is_empty() {
            break;
        }

        phases.push(ready.iter().map(|&i| tasks[i].0.clone()).collect());

        for &i in &ready {
            completed.insert(i);
            for &dependent in &reverse_adj[i] {
                in_degree[dependent] = in_degree[dependent].saturating_sub(1);
            }
        }
    }

    phases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_cycle() {
        let tasks = vec![
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ];
        let err = detect_cycle(&tasks).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("circular dependency"), "msg: {msg}");
        assert!(msg.contains("a") && msg.contains("b"), "msg: {msg}");
    }

    #[test]
    fn detects_three_node_cycle() {
        let tasks = vec![
            ("a".to_string(), vec!["c".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        let err = detect_cycle(&tasks).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("circular dependency"), "msg: {msg}");
    }

    #[test]
    fn accepts_dag() {
        let tasks = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["a".to_string()]),
            ("d".to_string(), vec!["b".to_string(), "c".to_string()]),
        ];
        assert!(detect_cycle(&tasks).is_ok());
    }

    #[test]
    fn accepts_independent_tasks() {
        let tasks = vec![("a".to_string(), vec![]), ("b".to_string(), vec![])];
        assert!(detect_cycle(&tasks).is_ok());
    }

    #[test]
    fn topological_phases_linear() {
        let tasks = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        let phases = topological_phases(&tasks);
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0], vec!["a"]);
        assert_eq!(phases[1], vec!["b"]);
        assert_eq!(phases[2], vec!["c"]);
    }

    #[test]
    fn topological_phases_diamond() {
        let tasks = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["a".to_string()]),
            ("d".to_string(), vec!["b".to_string(), "c".to_string()]),
        ];
        let phases = topological_phases(&tasks);
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0], vec!["a"]);
        assert!(phases[1].contains(&"b".to_string()));
        assert!(phases[1].contains(&"c".to_string()));
        assert_eq!(phases[2], vec!["d"]);
    }

    #[test]
    fn topological_phases_independent() {
        let tasks = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec![]),
            ("c".to_string(), vec![]),
        ];
        let phases = topological_phases(&tasks);
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].len(), 3);
    }
}
