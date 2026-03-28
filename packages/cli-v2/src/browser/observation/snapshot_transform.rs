//! Snapshot data transformation: CDP AX tree → §10.1 spec output.
//!
//! These functions are pure logic (no browser/CDP dependency) and are unit-tested.
//!
//! Contract per api-reference.md §10.1:
//! - `format`: always "snapshot"
//! - `content`: string with `[ref=eN]` labels, one node per line
//! - `nodes`: array with ref/role/name/value fields
//! - `stats`: node_count / interactive_count

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A normalised accessibility node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AXNode {
    /// Stable reference label, e.g. "e1", "e2", ...
    pub ref_id: String,
    /// ARIA role string (e.g. "button", "textbox")
    pub role: String,
    /// Accessible name
    pub name: String,
    /// Current value (inputs, text areas); empty string if not applicable
    pub value: String,
    /// Whether this node is considered interactive
    pub interactive: bool,
    /// Tree depth (0 = root)
    pub depth: usize,
    /// Children
    pub children: Vec<AXNode>,
}

/// Options that control snapshot output.
#[derive(Debug, Clone, Default)]
pub struct SnapshotOptions {
    /// Include only interactive nodes
    pub interactive: bool,
    /// Remove empty structural nodes
    pub compact: bool,
    /// Maximum tree depth (None = unlimited)
    pub depth: Option<usize>,
    /// CSS selector to limit subtree (None = whole page)
    pub selector: Option<String>,
}

/// Snapshot output ready to serialise as §10.1 data.
#[derive(Debug, Clone)]
pub struct SnapshotOutput {
    pub content: String,
    pub nodes: Vec<NodeEntry>,
    pub node_count: usize,
    pub interactive_count: usize,
}

/// Flat node entry for the `data.nodes` array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeEntry {
    pub r#ref: String,
    pub role: String,
    pub name: String,
    pub value: String,
}

/// Roles considered interactive per §10.1.
pub fn is_interactive_role(role: &str) -> bool {
    matches!(
        role,
        "button"
            | "checkbox"
            | "combobox"
            | "link"
            | "listbox"
            | "menuitem"
            | "menuitemcheckbox"
            | "menuitemradio"
            | "option"
            | "radio"
            | "searchbox"
            | "slider"
            | "spinbutton"
            | "switch"
            | "tab"
            | "textbox"
            | "treeitem"
    )
}

/// Filter: keep only interactive nodes (and their ancestors for context).
/// In flat-list context: simply keep nodes where `interactive == true`.
pub fn filter_interactive(nodes: Vec<AXNode>) -> Vec<AXNode> {
    nodes.into_iter().filter(|n| n.interactive).collect()
}

/// Filter: remove empty structural nodes (role is generic/none and name is empty).
pub fn filter_compact(nodes: Vec<AXNode>) -> Vec<AXNode> {
    nodes
        .into_iter()
        .filter(|n| {
            // Keep if has a meaningful role or non-empty name
            !matches!(n.role.as_str(), "generic" | "none" | "") || !n.name.is_empty()
        })
        .collect()
}

/// Filter: keep only nodes up to the given maximum depth.
pub fn apply_depth(nodes: Vec<AXNode>, max_depth: usize) -> Vec<AXNode> {
    nodes.into_iter().filter(|n| n.depth <= max_depth).collect()
}

/// Filter: keep only nodes belonging to the subtree rooted at `selector`.
///
/// CSS selector → AX subtree matching requires a nodeId lookup via CDP, which is
/// performed in `execute()` before calling this function. The matching node IDs are
/// passed in as `allowed_ref_ids`. An empty set means selector matched nothing —
/// return an empty list.
///
/// When called from `parse_ax_tree` without DOM context, `allowed_ref_ids` is empty
/// and the filter is a no-op (all nodes returned) until execute() wires the subtree.
pub fn apply_selector(nodes: Vec<AXNode>, allowed_ref_ids: &[String]) -> Vec<AXNode> {
    if allowed_ref_ids.is_empty() {
        // No selector resolved yet (pure parse context) — return all nodes unchanged.
        return nodes;
    }
    nodes
        .into_iter()
        .filter(|n| allowed_ref_ids.contains(&n.ref_id))
        .collect()
}

/// Render a flat node list to `content` string with `[ref=eN]` labels.
/// Format per §10.1: `- role "name" [ref=eN]` with depth-based indentation.
pub fn render_content(nodes: &[AXNode]) -> String {
    let mut lines = Vec::new();
    for node in nodes {
        let indent = "  ".repeat(node.depth);
        let mut line = format!(
            "{indent}- {} \"{}\" [ref={}]",
            node.role, node.name, node.ref_id
        );
        if !node.value.is_empty() {
            line.push_str(&format!(" value=\"{}\"", node.value));
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Build stats from a flat node list.
pub fn build_stats(nodes: &[AXNode]) -> (usize, usize) {
    let node_count = nodes.len();
    let interactive_count = nodes.iter().filter(|n| n.interactive).count();
    (node_count, interactive_count)
}

/// Parse CDP Accessibility.getFullAXTree response into a flat AXNode list.
///
/// The CDP response has shape:
/// ```json
/// { "result": { "nodes": [ { "nodeId": "1", "role": {"value":"button"}, "name": {"value":"Submit"}, ... } ] } }
/// ```
pub fn parse_ax_tree(response: &Value, options: &SnapshotOptions) -> Vec<AXNode> {
    let nodes_json = response["result"]["nodes"].as_array();
    let Some(nodes_json) = nodes_json else {
        return vec![];
    };

    let mut counter = 0usize;
    let mut result = Vec::new();

    // Build a flat list — CDP provides nodes in tree order
    for node_json in nodes_json {
        let role = node_json["role"]["value"]
            .as_str()
            .unwrap_or("generic")
            .to_string();
        let name = node_json["name"]["value"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let value = node_json["value"]["value"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let interactive = is_interactive_role(&role);

        // Depth from parent chain would require tree traversal;
        // use CDP-provided implicit depth via ignored/hidden flags for now.
        // Implementation will compute actual depth from parentId chain.
        let depth = 0; // placeholder — implementation fills real depth

        counter += 1;
        result.push(AXNode {
            ref_id: format!("e{counter}"),
            role,
            name,
            value,
            interactive,
            depth,
            children: vec![],
        });
    }

    // Apply options
    if options.interactive {
        result = filter_interactive(result);
    }
    if options.compact {
        result = filter_compact(result);
    }
    if let Some(max_depth) = options.depth {
        result = apply_depth(result, max_depth);
    }

    result
}

/// Build the full SnapshotOutput from a flat node list.
pub fn build_output(nodes: Vec<AXNode>) -> SnapshotOutput {
    let content = render_content(&nodes);
    let (node_count, interactive_count) = build_stats(&nodes);
    let entries = nodes
        .iter()
        .map(|n| NodeEntry {
            r#ref: n.ref_id.clone(),
            role: n.role.clone(),
            name: n.name.clone(),
            value: n.value.clone(),
        })
        .collect();
    SnapshotOutput {
        content,
        nodes: entries,
        node_count,
        interactive_count,
    }
}

// ── Unit Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(ref_id: &str, role: &str, name: &str, interactive: bool, depth: usize) -> AXNode {
        AXNode {
            ref_id: ref_id.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            value: String::new(),
            interactive,
            depth,
            children: vec![],
        }
    }

    fn make_node_with_value(
        ref_id: &str,
        role: &str,
        name: &str,
        value: &str,
        interactive: bool,
        depth: usize,
    ) -> AXNode {
        AXNode {
            ref_id: ref_id.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            value: value.to_string(),
            interactive,
            depth,
            children: vec![],
        }
    }

    // ── is_interactive_role ──────────────────────────────────────────

    #[test]
    fn test_interactive_roles() {
        assert!(is_interactive_role("button"));
        assert!(is_interactive_role("textbox"));
        assert!(is_interactive_role("link"));
        assert!(is_interactive_role("checkbox"));
        assert!(is_interactive_role("combobox"));
    }

    #[test]
    fn test_non_interactive_roles() {
        assert!(!is_interactive_role("generic"));
        assert!(!is_interactive_role("none"));
        assert!(!is_interactive_role("heading"));
        assert!(!is_interactive_role("paragraph"));
        assert!(!is_interactive_role(""));
    }

    // ── filter_interactive ───────────────────────────────────────────

    #[test]
    fn test_filter_interactive_keeps_only_interactive() {
        let nodes = vec![
            make_node("e1", "button", "Submit", true, 0),
            make_node("e2", "heading", "Title", false, 0),
            make_node("e3", "textbox", "Search", true, 0),
            make_node("e4", "paragraph", "Text", false, 0),
        ];
        let result = filter_interactive(nodes);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e3");
    }

    #[test]
    fn test_filter_interactive_empty_list() {
        let result = filter_interactive(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_interactive_all_non_interactive() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "Text", false, 0),
        ];
        let result = filter_interactive(nodes);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_interactive_all_interactive() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "link", "Home", true, 0),
        ];
        let result = filter_interactive(nodes.clone());
        assert_eq!(result.len(), 2);
    }

    // ── filter_compact ───────────────────────────────────────────────

    #[test]
    fn test_filter_compact_removes_empty_structural() {
        let nodes = vec![
            make_node("e1", "generic", "", false, 0), // empty structural — remove
            make_node("e2", "button", "OK", true, 0), // has name — keep
            make_node("e3", "none", "", false, 0),    // empty structural — remove
            make_node("e4", "generic", "Container", false, 0), // has name — keep
        ];
        let result = filter_compact(nodes);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e2");
        assert_eq!(result[1].ref_id, "e4");
    }

    #[test]
    fn test_filter_compact_keeps_meaningful_nodes() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "", false, 0), // paragraph with no name — keep (has role)
        ];
        let result = filter_compact(nodes);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_compact_empty_list() {
        let result = filter_compact(vec![]);
        assert!(result.is_empty());
    }

    // ── apply_depth ──────────────────────────────────────────────────

    #[test]
    fn test_apply_depth_limits_to_max() {
        let nodes = vec![
            make_node("e1", "generic", "root", false, 0),
            make_node("e2", "button", "OK", true, 1),
            make_node("e3", "link", "Home", true, 2),
            make_node("e4", "button", "Deep", true, 3),
        ];
        let result = apply_depth(nodes, 1);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].depth, 0);
        assert_eq!(result[1].depth, 1);
    }

    #[test]
    fn test_apply_depth_zero_returns_root_only() {
        let nodes = vec![
            make_node("e1", "generic", "root", false, 0),
            make_node("e2", "button", "OK", true, 1),
        ];
        let result = apply_depth(nodes, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ref_id, "e1");
    }

    #[test]
    fn test_apply_depth_large_keeps_all() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "button", "B", true, 5),
            make_node("e3", "button", "C", true, 10),
        ];
        let result = apply_depth(nodes, 100);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_apply_depth_empty_list() {
        let result = apply_depth(vec![], 5);
        assert!(result.is_empty());
    }

    // ── apply_selector ───────────────────────────────────────────────

    #[test]
    fn test_apply_selector_empty_allowed_ids_returns_all() {
        // No DOM context: allowed_ref_ids is empty → no-op, all nodes returned.
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let result = apply_selector(nodes, &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_selector_filters_to_allowed_ids() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
            make_node("e3", "heading", "C", false, 0),
        ];
        let allowed = vec!["e1".to_string(), "e3".to_string()];
        let result = apply_selector(nodes, &allowed);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ref_id, "e1");
        assert_eq!(result[1].ref_id, "e3");
    }

    #[test]
    fn test_apply_selector_no_match_returns_empty() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let allowed = vec!["e99".to_string()];
        let result = apply_selector(nodes, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_selector_empty_list() {
        let result = apply_selector(vec![], &["e1".to_string()]);
        assert!(result.is_empty());
    }

    // ── render_content ───────────────────────────────────────────────

    #[test]
    fn test_render_content_basic() {
        let nodes = vec![
            make_node("e1", "textbox", "Search", true, 0),
            make_node("e2", "button", "Google Search", true, 0),
        ];
        let content = render_content(&nodes);
        assert!(content.contains("[ref=e1]"), "must contain [ref=e1]");
        assert!(content.contains("[ref=e2]"), "must contain [ref=e2]");
        assert!(content.contains("textbox"), "must contain role");
        assert!(content.contains("Search"), "must contain name");
    }

    #[test]
    fn test_render_content_indentation() {
        let nodes = vec![
            make_node("e1", "generic", "Container", false, 0),
            make_node("e2", "button", "OK", true, 1),
            make_node("e3", "button", "Cancel", true, 2),
        ];
        let content = render_content(&nodes);
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        // depth 0: no indent
        assert!(lines[0].starts_with("- "), "depth 0 has no indent");
        // depth 1: 2 spaces
        assert!(lines[1].starts_with("  - "), "depth 1 has 2-space indent");
        // depth 2: 4 spaces
        assert!(lines[2].starts_with("    - "), "depth 2 has 4-space indent");
    }

    #[test]
    fn test_render_content_includes_value() {
        let nodes = vec![make_node_with_value(
            "e1",
            "textbox",
            "Email",
            "user@example.com",
            true,
            0,
        )];
        let content = render_content(&nodes);
        assert!(
            content.contains("value=\"user@example.com\""),
            "must include value when present"
        );
    }

    #[test]
    fn test_render_content_empty_list() {
        let content = render_content(&[]);
        assert!(content.is_empty(), "empty node list produces empty content");
    }

    #[test]
    fn test_render_content_ref_labels_format() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e42", "link", "B", true, 0),
        ];
        let content = render_content(&nodes);
        assert!(content.contains("[ref=e1]"));
        assert!(content.contains("[ref=e42]"));
    }

    // ── build_stats ──────────────────────────────────────────────────

    #[test]
    fn test_build_stats_counts_correctly() {
        let nodes = vec![
            make_node("e1", "button", "OK", true, 0),
            make_node("e2", "heading", "Title", false, 0),
            make_node("e3", "textbox", "Search", true, 0),
            make_node("e4", "paragraph", "Text", false, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 4);
        assert_eq!(interactive_count, 2);
    }

    #[test]
    fn test_build_stats_empty_list() {
        let (node_count, interactive_count) = build_stats(&[]);
        assert_eq!(node_count, 0);
        assert_eq!(interactive_count, 0);
    }

    #[test]
    fn test_build_stats_all_interactive() {
        let nodes = vec![
            make_node("e1", "button", "A", true, 0),
            make_node("e2", "link", "B", true, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 2);
        assert_eq!(interactive_count, 2);
    }

    #[test]
    fn test_build_stats_none_interactive() {
        let nodes = vec![
            make_node("e1", "heading", "Title", false, 0),
            make_node("e2", "paragraph", "Text", false, 0),
        ];
        let (node_count, interactive_count) = build_stats(&nodes);
        assert_eq!(node_count, 2);
        assert_eq!(interactive_count, 0);
    }

    // ── build_output ─────────────────────────────────────────────────

    #[test]
    fn test_build_output_complete() {
        let nodes = vec![
            make_node("e1", "textbox", "Search", true, 0),
            make_node("e2", "button", "Go", true, 0),
        ];
        let output = build_output(nodes);
        assert_eq!(output.node_count, 2);
        assert_eq!(output.interactive_count, 2);
        assert!(output.content.contains("[ref=e1]"));
        assert!(output.content.contains("[ref=e2]"));
        assert_eq!(output.nodes.len(), 2);
        assert_eq!(output.nodes[0].r#ref, "e1");
        assert_eq!(output.nodes[0].role, "textbox");
        assert_eq!(output.nodes[0].name, "Search");
    }

    #[test]
    fn test_build_output_node_entries_have_required_fields() {
        let nodes = vec![make_node_with_value(
            "e1",
            "textbox",
            "Email",
            "test@test.com",
            true,
            0,
        )];
        let output = build_output(nodes);
        let entry = &output.nodes[0];
        assert_eq!(entry.r#ref, "e1");
        assert_eq!(entry.role, "textbox");
        assert_eq!(entry.name, "Email");
        assert_eq!(entry.value, "test@test.com");
    }

    // ── parse_ax_tree ────────────────────────────────────────────────

    #[test]
    fn test_parse_ax_tree_basic() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "Submit"} },
                    { "nodeId": "2", "role": {"value": "textbox"}, "name": {"value": "Email"} },
                ]
            }
        });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].ref_id, "e1");
        assert_eq!(nodes[0].role, "button");
        assert_eq!(nodes[0].name, "Submit");
        assert!(nodes[0].interactive);
        assert_eq!(nodes[1].ref_id, "e2");
        assert_eq!(nodes[1].role, "textbox");
    }

    #[test]
    fn test_parse_ax_tree_empty_response() {
        let response = serde_json::json!({ "result": { "nodes": [] } });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ax_tree_missing_nodes() {
        let response = serde_json::json!({ "result": {} });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ax_tree_interactive_filter() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "Submit"} },
                    { "nodeId": "2", "role": {"value": "heading"}, "name": {"value": "Title"} },
                    { "nodeId": "3", "role": {"value": "link"}, "name": {"value": "Home"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            interactive: true,
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.interactive));
    }

    #[test]
    fn test_parse_ax_tree_compact_filter() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "generic"}, "name": {"value": ""} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "OK"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            compact: true,
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, "button");
    }

    #[test]
    fn test_parse_ax_tree_depth_filter() {
        // TDD: defines the expected contract after real depth computation from parentId
        // chains is implemented.
        //
        // Current stub assigns depth=0 to all nodes, so apply_depth(_, 0) keeps all
        // nodes (0 <= 0). This test asserts the CONTRACT: with depth=Some(0) only the
        // root-level node should survive. It currently FAILS — that is intentional and
        // makes the placeholder gap visible for the implementer.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "generic"}, "name": {"value": "root"} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "OK"} },
                    { "nodeId": "3", "role": {"value": "link"}, "name": {"value": "Home"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            depth: Some(0),
            ..Default::default()
        };
        let nodes = parse_ax_tree(&response, &opts);
        // After real depth impl: only root (depth=0) survives → 1 node.
        // With placeholder (all depth=0): all 3 pass (0 <= 0) → returns 3.
        assert_eq!(
            nodes.len(),
            1,
            "depth=0 must return only root node; got {} (stub assigns all depth=0)",
            nodes.len()
        );
    }

    #[test]
    fn test_parse_ax_tree_selector_option_accepted() {
        // selector filtering via apply_selector() requires DOM context (nodeId lookup)
        // which is wired in execute(), not in parse_ax_tree. This UT verifies parse_ax_tree
        // accepts the selector option without panicking (no-op in pure parse context).
        // Pure apply_selector() contract is tested separately (test_apply_selector_*).
        // E2E subtree-limiting behaviour is covered by snap_selector_flag_limits_subtree.
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "OK"} },
                    { "nodeId": "2", "role": {"value": "link"}, "name": {"value": "Home"} },
                ]
            }
        });
        let opts = SnapshotOptions {
            selector: Some("body".to_string()),
            ..Default::default()
        };
        // Must not panic; actual subtree filtering handled in execute() via CDP node lookup
        let nodes = parse_ax_tree(&response, &opts);
        assert!(
            nodes.len() <= 2,
            "selector option must not expand node list"
        );
    }

    #[test]
    fn test_parse_ax_tree_assigns_sequential_refs() {
        let response = serde_json::json!({
            "result": {
                "nodes": [
                    { "nodeId": "1", "role": {"value": "button"}, "name": {"value": "A"} },
                    { "nodeId": "2", "role": {"value": "button"}, "name": {"value": "B"} },
                    { "nodeId": "3", "role": {"value": "button"}, "name": {"value": "C"} },
                ]
            }
        });
        let nodes = parse_ax_tree(&response, &SnapshotOptions::default());
        assert_eq!(nodes[0].ref_id, "e1");
        assert_eq!(nodes[1].ref_id, "e2");
        assert_eq!(nodes[2].ref_id, "e3");
    }
}
