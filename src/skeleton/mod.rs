use anyhow::Result;
use ignore::WalkBuilder;
use std::path::Path;

/// Generate a tree-like skeleton map of the project structure
pub async fn generate_skeleton(root: &str) -> Result<String> {
    let root_path = Path::new(root).canonicalize()?;
    let root_name = root_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    // Collect all files respecting .gitignore
    let mut entries: Vec<(String, bool)> = Vec::new();

    let walker = WalkBuilder::new(&root_path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();

        // Skip the root itself
        if path == root_path {
            continue;
        }

        // Get relative path
        if let Ok(relative) = path.strip_prefix(&root_path) {
            let relative_str = relative.to_string_lossy().to_string();
            let is_dir = path.is_dir();
            entries.push((relative_str, is_dir));
        }
    }

    // Sort entries for consistent output
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // Build tree structure
    let mut result = format!("{}/\n", root_name);

    for (i, (path, is_dir)) in entries.iter().enumerate() {
        let depth = path.matches(['/', '\\']).count();
        let is_last = is_last_at_depth(&entries, i, depth);

        let prefix = build_prefix(&entries, i, depth);
        let connector = if is_last { "└── " } else { "├── " };

        let name = Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let suffix = if *is_dir { "/" } else { "" };

        result.push_str(&format!("{}{}{}{}\n", prefix, connector, name, suffix));
    }

    Ok(result)
}

fn is_last_at_depth(entries: &[(String, bool)], current_idx: usize, depth: usize) -> bool {
    let current_parent = get_parent(&entries[current_idx].0);

    for (path, _) in entries.iter().skip(current_idx + 1) {
        let entry_depth = path.matches(['/', '\\']).count();
        let entry_parent = get_parent(path);

        if entry_depth == depth && entry_parent == current_parent {
            return false;
        }
        if entry_depth < depth {
            break;
        }
    }
    true
}

fn get_parent(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn build_prefix(entries: &[(String, bool)], current_idx: usize, depth: usize) -> String {
    let mut prefix = String::new();

    for d in 0..depth {
        // Check if there are more siblings at this depth level
        let ancestor_path = get_ancestor(&entries[current_idx].0, d);
        let has_more_siblings = entries
            .iter()
            .skip(current_idx + 1)
            .any(|(path, _)| {
                let path_depth = path.matches(['/', '\\']).count();
                path_depth >= d && get_ancestor(path, d) == ancestor_path && {
                    // Check if there's a sibling at exactly depth d
                    entries.iter().skip(current_idx + 1).any(|(p, _)| {
                        let pd = p.matches(['/', '\\']).count();
                        pd == d && get_parent(p) == get_parent(&ancestor_path)
                    })
                }
            });

        if has_more_siblings {
            prefix.push_str("│   ");
        } else {
            prefix.push_str("    ");
        }
    }

    prefix
}

fn get_ancestor(path: &str, depth: usize) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    if depth < parts.len() {
        parts[..=depth].join("/")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_skeleton() {
        // This test requires an actual directory structure
        // For now just verify it doesn't panic on current directory
        let result = generate_skeleton(".").await;
        assert!(result.is_ok());
    }
}
