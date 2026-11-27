//! Filter, sort, and group evaluation over a note set.
//!
//! A [`Query`] describes how to transform a set of notes into a view:
//! filter → sort → group. The evaluation is pure and operates on
//! [`PropertyValue`] maps, so it can be tested without the GUI.

use std::collections::{BTreeMap, HashMap};

use crate::properties::{PropertyMap, PropertyValue};
use serde::{Deserialize, Serialize};

/// A note path paired with its properties.
pub type NoteEntry = (String, PropertyMap);

/// Notes keyed by path.
pub type NoteSet = HashMap<String, PropertyMap>;

/// A filter condition on a single property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum FilterOp {
    /// Value equals the given value.
    Is { value: PropertyValue },
    /// Value does not equal the given value.
    IsNot { value: PropertyValue },
    /// Text contains substring (case-insensitive).
    Contains { value: String },
    /// Text does not contain substring.
    NotContains { value: String },
    /// Date is before the given date.
    Before { value: PropertyValue },
    /// Date is after the given date.
    After { value: PropertyValue },
    /// Property is empty (unset, empty string, false, NaN, empty array).
    IsEmpty,
    /// Property is not empty.
    IsNotEmpty,
    /// Value is one of the given options (for select/multi-select).
    In { values: Vec<PropertyValue> },
    /// Value is not one of the given options.
    NotIn { values: Vec<PropertyValue> },
}

/// A filter group: all conditions must match (AND) or any can match (OR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterGroup {
    /// Whether all conditions must match (true) or any can match (false).
    pub all: bool,
    /// Child conditions.
    pub conditions: Vec<FilterCondition>,
    /// Nested groups.
    pub groups: Vec<FilterGroup>,
}

impl Default for FilterGroup {
    fn default() -> Self {
        Self {
            all: true,
            conditions: Vec::new(),
            groups: Vec::new(),
        }
    }
}

/// A single filter condition: property + operator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterCondition {
    /// Property name (canonical).
    pub property: String,
    /// The operator and its argument.
    pub op: FilterOp,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A sort key: property + direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortKey {
    /// Property to sort by.
    pub property: String,
    /// Direction.
    pub direction: SortDirection,
}

/// Group-by specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupBy {
    /// Property to group by.
    pub property: String,
    /// Whether to show empty groups.
    pub show_empty: bool,
    /// Maximum number of groups (0 = unlimited).
    pub limit: usize,
}

/// Complete query: filter → sort → group.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Query {
    /// Filter to apply first.
    pub filter: FilterGroup,
    /// Sort keys (applied in order).
    pub sort: Vec<SortKey>,
    /// Optional grouping.
    pub group_by: Option<GroupBy>,
    /// Properties to include in results (empty = all).
    pub visible_properties: Vec<String>,
    /// Maximum number of results (0 = unlimited).
    pub limit: usize,
}

impl Query {
    /// Create an empty query (matches everything).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate the query against a set of notes.
    ///
    /// `notes` maps note paths to their property maps.
    /// Returns a [`QueryResult`] with filtered, sorted, and grouped notes.
    #[must_use]
    pub fn evaluate(&self, notes: &NoteSet) -> QueryResult {
        // Step 1: Filter
        let filtered: HashMap<_, _> = notes
            .iter()
            .filter(|(_, props)| self.filter.matches(props))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let total_matched = filtered.len();

        // Step 2: Sort
        let mut sorted: Vec<_> = filtered.into_iter().collect();
        if !self.sort.is_empty() {
            sorted.sort_by(|a, b| {
                for key in &self.sort {
                    let cmp = compare_props(
                        a.1.get(&key.property),
                        b.1.get(&key.property),
                        key.direction,
                    );
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                // Stable fallback: by path
                a.0.cmp(&b.0)
            });
        }

        // Step 3: Limit
        if self.limit > 0 && sorted.len() > self.limit {
            sorted.truncate(self.limit);
        }

        // Step 4: Group (if requested)
        let grouped = if let Some(group_by) = &self.group_by {
            let mut groups: BTreeMap<String, Vec<NoteEntry>> = BTreeMap::new();
            for (path, props) in &sorted {
                let group_key = group_key_for(props.get(&group_by.property), group_by.show_empty);
                groups
                    .entry(group_key)
                    .or_default()
                    .push((path.clone(), props.clone()));
            }
            Some(
                groups
                    .into_iter()
                    .map(|(key, notes)| Group { key, notes })
                    .collect(),
            )
        } else {
            None
        };

        QueryResult {
            notes: sorted,
            groups: grouped,
            total_matched,
        }
    }
}

/// Result of a query evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    /// Filtered and sorted notes (flat list).
    pub notes: Vec<NoteEntry>,
    /// Grouped notes (if `group_by` was specified).
    pub groups: Option<Vec<Group>>,
    /// Total notes in the source (before limit).
    pub total_matched: usize,
}

/// A group of notes sharing a group key.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The group key (displayable).
    pub key: String,
    /// Notes in this group.
    pub notes: Vec<NoteEntry>,
}

impl FilterGroup {
    /// Check if a property map matches this filter group.
    fn matches(&self, props: &PropertyMap) -> bool {
        let cond_match = if self.all {
            self.conditions.iter().all(|c| c.matches(props))
        } else {
            self.conditions.iter().any(|c| c.matches(props))
        };
        let group_match = if self.all {
            self.groups.iter().all(|g| g.matches(props))
        } else {
            self.groups.iter().any(|g| g.matches(props))
        };
        if self.all {
            cond_match && group_match
        } else {
            cond_match || group_match
        }
    }
}

impl FilterCondition {
    /// Check if a property map matches this condition.
    fn matches(&self, props: &PropertyMap) -> bool {
        let value = props.get(&self.property);
        match &self.op {
            FilterOp::Is { value: target } => value.is_some_and(|v| v == target),
            FilterOp::IsNot { value: target } => value.is_none_or(|v| v != target),
            FilterOp::Contains { value: needle } => {
                if let Some(PropertyValue::Text(text)) = value {
                    text.to_lowercase().contains(&needle.to_lowercase())
                } else {
                    false
                }
            }
            FilterOp::NotContains { value: needle } => {
                if let Some(PropertyValue::Text(text)) = value {
                    !text.to_lowercase().contains(&needle.to_lowercase())
                } else {
                    true
                }
            }
            FilterOp::Before { value: target } => {
                if let (Some(PropertyValue::Date(Some(a))), PropertyValue::Date(Some(b))) =
                    (value, target)
                {
                    a < b
                } else {
                    false
                }
            }
            FilterOp::After { value: target } => {
                if let (Some(PropertyValue::Date(Some(a))), PropertyValue::Date(Some(b))) =
                    (value, target)
                {
                    a > b
                } else {
                    false
                }
            }
            FilterOp::IsEmpty => value.is_none_or(PropertyValue::is_empty),
            FilterOp::IsNotEmpty => value.is_some_and(|v| !v.is_empty()),
            FilterOp::In { values } => value.is_some_and(|v| values.contains(v)),
            FilterOp::NotIn { values } => value.is_none_or(|v| !values.contains(v)),
        }
    }
}

/// Compare two optional property values for sorting.
fn compare_props(
    a: Option<&PropertyValue>,
    b: Option<&PropertyValue>,
    direction: SortDirection,
) -> std::cmp::Ordering {
    let ord = match (a, b) {
        (Some(a), Some(b)) => a.sort_key().cmp(&b.sort_key()),
        (Some(_), None) => std::cmp::Ordering::Less, // values sort before empty
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    match direction {
        SortDirection::Asc => ord,
        SortDirection::Desc => ord.reverse(),
    }
}

/// Get the group key for a property value.
fn group_key_for(value: Option<&PropertyValue>, show_empty: bool) -> String {
    match value {
        Some(v) if !v.is_empty() => v.display(),
        None | Some(_) if show_empty => "(empty)".to_string(),
        _ => "(ungrouped)".to_string(),
    }
}

/// Parse a filter expression from a simple string syntax.
///
/// Supported:
/// - `prop:value` → prop is value
/// - `prop:"text"` → prop contains text
/// - `prop:>2024-01-01` → prop after date
/// - `prop:<2024-01-01` → prop before date
/// - `prop:!value` → prop is not value
/// - `-prop:value` → exclude (NOT)
/// - `tag:#tag` → tag filter
///
/// This is a simple parser for the search bar; complex filters use the
/// structured `FilterGroup` directly.
#[must_use]
pub fn parse_simple_filter(expr: &str) -> Option<FilterCondition> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Check for a negation prefix.
    let (negated, expr) = if let Some(rest) = expr.strip_prefix('-') {
        (true, rest)
    } else {
        (false, expr)
    };

    // Find the operator.
    let (prop, op_str, value_str) = if let Some(idx) = expr.find(':') {
        let prop = &expr[..idx];
        let rest = &expr[idx + 1..];
        if let Some(rest) = rest.strip_prefix('>') {
            (prop, ">", rest)
        } else if let Some(rest) = rest.strip_prefix('<') {
            (prop, "<", rest)
        } else if let Some(rest) = rest.strip_prefix('!') {
            (prop, "!", rest)
        } else if let Some(inner) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            (prop, "~", inner)
        } else {
            (prop, "=", rest)
        }
    } else if let Some((_, tag)) = expr.split_once('#') {
        // Tag shorthand: #tag
        ("tags", "=", tag)
    } else {
        return None;
    };

    let op = if negated {
        match op_str {
            "=" | "!" => FilterOp::IsNot {
                value: value_str.parse().ok()?,
            },
            "~" => FilterOp::NotContains {
                value: value_str.to_string(),
            },
            ">" => FilterOp::After {
                value: value_str.parse().ok()?,
            },
            "<" => FilterOp::Before {
                value: value_str.parse().ok()?,
            },
            _ => return None,
        }
    } else {
        match op_str {
            "=" => FilterOp::Is {
                value: value_str.parse().ok()?,
            },
            "~" => FilterOp::Contains {
                value: value_str.to_string(),
            },
            ">" => FilterOp::After {
                value: value_str.parse().ok()?,
            },
            "<" => FilterOp::Before {
                value: value_str.parse().ok()?,
            },
            "!" => FilterOp::IsNot {
                value: value_str.parse().ok()?,
            },
            _ => return None,
        }
    };

    Some(FilterCondition {
        property: prop.to_string(),
        op,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::PropertyValue;
    use std::collections::HashMap;

    fn props(kv: &[(&str, PropertyValue)]) -> HashMap<String, PropertyValue> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn notes(
        kv: &[(&str, HashMap<String, PropertyValue>)],
    ) -> HashMap<String, HashMap<String, PropertyValue>> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn filter_is_matches() {
        let filter = FilterCondition {
            property: "status".to_string(),
            op: FilterOp::Is {
                value: PropertyValue::Select("done".to_string()),
            },
        };
        assert!(filter.matches(&props(&[(
            "status",
            PropertyValue::Select("done".to_string())
        )])));
        assert!(!filter.matches(&props(&[(
            "status",
            PropertyValue::Select("todo".to_string())
        )])));
        assert!(!filter.matches(&props(&[])));
    }

    #[test]
    fn filter_is_not_matches() {
        let filter = FilterCondition {
            property: "status".to_string(),
            op: FilterOp::IsNot {
                value: PropertyValue::Select("done".to_string()),
            },
        };
        assert!(!filter.matches(&props(&[(
            "status",
            PropertyValue::Select("done".to_string())
        )])));
        assert!(filter.matches(&props(&[(
            "status",
            PropertyValue::Select("todo".to_string())
        )])));
        assert!(filter.matches(&props(&[])));
    }

    #[test]
    fn filter_contains_text() {
        let filter = FilterCondition {
            property: "title".to_string(),
            op: FilterOp::Contains {
                value: "meeting".to_string(),
            },
        };
        assert!(filter.matches(&props(&[(
            "title",
            PropertyValue::Text("Team Meeting".to_string())
        )])));
        assert!(filter.matches(&props(&[(
            "title",
            PropertyValue::Text("MEETING notes".to_string())
        )])));
        assert!(!filter.matches(&props(&[(
            "title",
            PropertyValue::Text("Notes".to_string())
        )])));
    }

    #[test]
    fn filter_date_before_after() {
        let filter_before = FilterCondition {
            property: "date".to_string(),
            op: FilterOp::Before {
                value: PropertyValue::Date(Some(
                    chrono::DateTime::parse_from_rfc3339("2024-01-15T00:00:00Z").unwrap(),
                )),
            },
        };
        let filter_after = FilterCondition {
            property: "date".to_string(),
            op: FilterOp::After {
                value: PropertyValue::Date(Some(
                    chrono::DateTime::parse_from_rfc3339("2024-01-15T00:00:00Z").unwrap(),
                )),
            },
        };
        let early = props(&[(
            "date",
            PropertyValue::Date(Some(
                chrono::DateTime::parse_from_rfc3339("2024-01-10T00:00:00Z").unwrap(),
            )),
        )]);
        let late = props(&[(
            "date",
            PropertyValue::Date(Some(
                chrono::DateTime::parse_from_rfc3339("2024-01-20T00:00:00Z").unwrap(),
            )),
        )]);
        assert!(filter_before.matches(&early));
        assert!(!filter_before.matches(&late));
        assert!(filter_after.matches(&late));
        assert!(!filter_after.matches(&early));
    }

    #[test]
    fn filter_is_empty() {
        let filter = FilterCondition {
            property: "status".to_string(),
            op: FilterOp::IsEmpty,
        };
        assert!(filter.matches(&props(&[])));
        assert!(filter.matches(&props(&[("status", PropertyValue::Text(String::new()))])));
        assert!(filter.matches(&props(&[("status", PropertyValue::Bool(false))])));
        assert!(!filter.matches(&props(&[(
            "status",
            PropertyValue::Text("done".to_string())
        )])));
    }

    #[test]
    fn filter_group_all() {
        let group = FilterGroup {
            all: true,
            conditions: vec![
                FilterCondition {
                    property: "status".to_string(),
                    op: FilterOp::Is {
                        value: PropertyValue::Select("done".to_string()),
                    },
                },
                FilterCondition {
                    property: "priority".to_string(),
                    op: FilterOp::Is {
                        value: PropertyValue::Number(1.0),
                    },
                },
            ],
            groups: vec![],
        };
        assert!(group.matches(&props(&[
            ("status", PropertyValue::Select("done".to_string())),
            ("priority", PropertyValue::Number(1.0)),
        ])));
        assert!(!group.matches(&props(&[
            ("status", PropertyValue::Select("done".to_string())),
            ("priority", PropertyValue::Number(2.0)),
        ])));
    }

    #[test]
    fn filter_group_any() {
        let group = FilterGroup {
            all: false,
            conditions: vec![
                FilterCondition {
                    property: "status".to_string(),
                    op: FilterOp::Is {
                        value: PropertyValue::Select("done".to_string()),
                    },
                },
                FilterCondition {
                    property: "status".to_string(),
                    op: FilterOp::Is {
                        value: PropertyValue::Select("archived".to_string()),
                    },
                },
            ],
            groups: vec![],
        };
        assert!(group.matches(&props(&[(
            "status",
            PropertyValue::Select("done".to_string())
        )])));
        assert!(group.matches(&props(&[(
            "status",
            PropertyValue::Select("archived".to_string())
        )])));
        assert!(!group.matches(&props(&[(
            "status",
            PropertyValue::Select("todo".to_string())
        )])));
    }

    #[test]
    fn query_filter_sort() {
        let query = Query {
            filter: FilterGroup {
                all: true,
                conditions: vec![FilterCondition {
                    property: "status".to_string(),
                    op: FilterOp::Is {
                        value: PropertyValue::Select("done".to_string()),
                    },
                }],
                groups: vec![],
            },
            sort: vec![SortKey {
                property: "priority".to_string(),
                direction: SortDirection::Asc,
            }],
            group_by: None,
            visible_properties: vec![],
            limit: 0,
        };
        let notes = notes(&[
            (
                "a.md",
                props(&[
                    ("status", PropertyValue::Select("done".to_string())),
                    ("priority", PropertyValue::Number(3.0)),
                ]),
            ),
            (
                "b.md",
                props(&[
                    ("status", PropertyValue::Select("done".to_string())),
                    ("priority", PropertyValue::Number(1.0)),
                ]),
            ),
            (
                "c.md",
                props(&[
                    ("status", PropertyValue::Select("todo".to_string())),
                    ("priority", PropertyValue::Number(1.0)),
                ]),
            ),
        ]);
        let result = query.evaluate(&notes);
        assert_eq!(result.notes.len(), 2);
        assert_eq!(result.notes[0].0, "b.md"); // priority 1 first
        assert_eq!(result.notes[1].0, "a.md"); // priority 3 second
    }

    #[test]
    fn query_group_by() {
        let query = Query {
            filter: FilterGroup::default(),
            sort: vec![SortKey {
                property: "status".to_string(),
                direction: SortDirection::Asc,
            }],
            group_by: Some(GroupBy {
                property: "status".to_string(),
                show_empty: false,
                limit: 0,
            }),
            visible_properties: vec![],
            limit: 0,
        };
        let notes = notes(&[
            (
                "a.md",
                props(&[("status", PropertyValue::Select("done".to_string()))]),
            ),
            (
                "b.md",
                props(&[("status", PropertyValue::Select("todo".to_string()))]),
            ),
            (
                "c.md",
                props(&[("status", PropertyValue::Select("done".to_string()))]),
            ),
        ]);
        let result = query.evaluate(&notes);
        let groups = result.groups.unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "done");
        assert_eq!(groups[0].notes.len(), 2);
        assert_eq!(groups[1].key, "todo");
        assert_eq!(groups[1].notes.len(), 1);
    }

    #[test]
    fn parse_simple_filter_equality() {
        let f = parse_simple_filter("status:done").unwrap();
        assert_eq!(f.property, "status");
        assert!(matches!(f.op, FilterOp::Is { .. }));
    }

    #[test]
    fn parse_simple_filter_contains() {
        let f = parse_simple_filter(r#"title:"meeting""#).unwrap();
        assert_eq!(f.property, "title");
        assert!(matches!(f.op, FilterOp::Contains { .. }));
    }

    #[test]
    fn parse_simple_filter_date_comparison() {
        let f = parse_simple_filter("date:>2024-01-01").unwrap();
        assert_eq!(f.property, "date");
        assert!(matches!(f.op, FilterOp::After { .. }));
        let f = parse_simple_filter("date:<2024-01-01").unwrap();
        assert!(matches!(f.op, FilterOp::Before { .. }));
    }

    #[test]
    fn parse_simple_filter_negation() {
        let f = parse_simple_filter("-status:done").unwrap();
        assert_eq!(f.property, "status");
        assert!(matches!(f.op, FilterOp::IsNot { .. }));
    }

    #[test]
    fn parse_simple_filter_tag() {
        let f = parse_simple_filter("#rust").unwrap();
        assert_eq!(f.property, "tags");
        assert!(matches!(f.op, FilterOp::Is { .. }));
    }

    #[test]
    fn sort_direction_desc() {
        let query = Query {
            filter: FilterGroup::default(),
            sort: vec![SortKey {
                property: "priority".to_string(),
                direction: SortDirection::Desc,
            }],
            group_by: None,
            visible_properties: vec![],
            limit: 0,
        };
        let notes = notes(&[
            ("a.md", props(&[("priority", PropertyValue::Number(1.0))])),
            ("b.md", props(&[("priority", PropertyValue::Number(3.0))])),
        ]);
        let result = query.evaluate(&notes);
        assert_eq!(result.notes[0].0, "b.md"); // 3 first (desc)
        assert_eq!(result.notes[1].0, "a.md"); // 1 second
    }
}
