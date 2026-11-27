//! View definitions and management.
//!
//! A View is a saved query with presentation settings (columns, group-by, etc.).
//! Views power the sidebar, canvas, and list presentations.

use std::collections::HashMap;

use crate::query::{FilterGroup, GroupBy, Query, SortDirection, SortKey};
use serde::{Deserialize, Serialize};

/// A saved view configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    /// Unique identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// The underlying query.
    pub query: Query,
    /// Visible columns (property names) in order.
    pub columns: Vec<String>,
    /// Whether to show the file path column.
    pub show_path: bool,
    /// Icon for the view (emoji or lucide name).
    pub icon: Option<String>,
}

impl View {
    /// Create a new view with a basic query.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            query: Query::new(),
            columns: Vec::new(),
            show_path: true,
            icon: None,
        }
    }

    /// Create a view from a query.
    #[must_use]
    pub fn from_query(id: impl Into<String>, name: impl Into<String>, query: Query) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            query,
            columns: Vec::new(),
            show_path: true,
            icon: None,
        }
    }

    /// Set the filter for this view.
    #[must_use]
    pub fn with_filter(mut self, filter: FilterGroup) -> Self {
        self.query.filter = filter;
        self
    }

    /// Add a sort key.
    #[must_use]
    pub fn with_sort(mut self, property: impl Into<String>, direction: SortDirection) -> Self {
        self.query.sort.push(SortKey {
            property: property.into(),
            direction,
        });
        self
    }

    /// Set group-by.
    #[must_use]
    pub fn with_group_by(mut self, property: impl Into<String>) -> Self {
        self.query.group_by = Some(GroupBy {
            property: property.into(),
            show_empty: false,
            limit: 0,
        });
        self
    }

    /// Set visible columns.
    #[must_use]
    pub fn with_columns(mut self, columns: Vec<String>) -> Self {
        self.columns = columns;
        self
    }
}

/// Registry of views for a vault.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewRegistry {
    /// Views by ID.
    pub views: HashMap<String, View>,
    /// Default view ID for each context (e.g., "sidebar", "canvas").
    pub defaults: HashMap<String, String>,
}

impl ViewRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a view.
    pub fn set(&mut self, view: View) {
        self.views.insert(view.id.clone(), view);
    }

    /// Get a view by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&View> {
        self.views.get(id)
    }

    /// Remove a view.
    pub fn remove(&mut self, id: &str) -> Option<View> {
        self.views.remove(id)
    }

    /// List all views.
    #[must_use]
    pub fn list(&self) -> Vec<&View> {
        self.views.values().collect()
    }

    /// Set the default view for a context.
    pub fn set_default(&mut self, context: impl Into<String>, view_id: impl Into<String>) {
        self.defaults.insert(context.into(), view_id.into());
    }

    /// Get the default view for a context.
    pub fn get_default(&self, context: &str) -> Option<&View> {
        self.defaults.get(context).and_then(|id| self.views.get(id))
    }
}

/// Built-in view templates for common use cases.
pub mod templates {
    use super::View;
    use crate::properties::PropertyValue;
    use crate::query::{FilterCondition, FilterGroup, FilterOp, Query, SortDirection};

    /// All notes, sorted by modified date.
    pub fn all_notes() -> View {
        View::new("all-notes", "All Notes")
            .with_sort("modified", SortDirection::Desc)
            .with_columns(vec!["title".to_string(), "modified".to_string()])
    }

    /// Notes with a specific tag.
    pub fn by_tag(tag: &str) -> View {
        let filter = FilterGroup {
            all: true,
            conditions: vec![FilterCondition {
                property: "tags".to_string(),
                op: FilterOp::Contains {
                    value: tag.to_string(),
                },
            }],
            groups: vec![],
        };
        View::from_query(
            format!("tag-{tag}"),
            format!("#{tag}"),
            Query {
                filter,
                ..Query::new()
            },
        )
        .with_sort("modified", SortDirection::Desc)
    }

    /// Notes with a specific property value.
    #[must_use]
    pub fn by_property(property: &str, value: PropertyValue) -> View {
        let label = value.display();
        let filter = FilterGroup {
            all: true,
            conditions: vec![FilterCondition {
                property: property.to_string(),
                op: FilterOp::Is { value },
            }],
            groups: vec![],
        };
        View::from_query(
            format!("property-{property}-{label}"),
            format!("{property} = {label}"),
            Query {
                filter,
                ..Query::new()
            },
        )
        .with_sort("modified", SortDirection::Desc)
    }

    /// Recent notes (last 7 days).
    pub fn recent() -> View {
        let week_ago = chrono::Utc::now() - chrono::Duration::days(7);
        let filter = FilterGroup {
            all: true,
            conditions: vec![FilterCondition {
                property: "modified".to_string(),
                op: FilterOp::After {
                    value: PropertyValue::Date(Some(week_ago.into())),
                },
            }],
            groups: vec![],
        };
        View::from_query(
            "recent",
            "Recent",
            Query {
                filter,
                ..Query::new()
            },
        )
        .with_sort("modified", SortDirection::Desc)
    }

    /// Tasks (notes with checkboxes/tasks).
    pub fn tasks() -> View {
        let filter = FilterGroup {
            all: true,
            conditions: vec![FilterCondition {
                property: "tasks".to_string(),
                op: FilterOp::IsNotEmpty,
            }],
            groups: vec![],
        };
        View::from_query(
            "tasks",
            "Tasks",
            Query {
                filter,
                ..Query::new()
            },
        )
        .with_sort("due", SortDirection::Asc)
    }

    /// Notes grouped by a property (e.g., status, area).
    #[must_use]
    pub fn grouped_by(property: &str) -> View {
        View::new(format!("grouped-{property}"), format!("By {property}"))
            .with_group_by(property)
            .with_sort(property, SortDirection::Asc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::PropertyValue;
    use crate::query::{FilterCondition, FilterGroup, FilterOp, SortDirection, SortKey};

    #[test]
    fn view_creation() {
        let view = View::new("test", "Test View");
        assert_eq!(view.id, "test");
        assert_eq!(view.name, "Test View");
        assert!(view.query.filter.conditions.is_empty());
    }

    #[test]
    fn view_with_filter() {
        let filter = FilterGroup {
            all: true,
            conditions: vec![FilterCondition {
                property: "status".to_string(),
                op: FilterOp::Is {
                    value: PropertyValue::Select("done".to_string()),
                },
            }],
            groups: vec![],
        };
        let view = View::new("done", "Done").with_filter(filter.clone());
        assert_eq!(view.query.filter, filter);
    }

    #[test]
    fn view_with_sort() {
        let view = View::new("sorted", "Sorted").with_sort("priority", SortDirection::Asc);
        assert_eq!(
            view.query.sort,
            vec![SortKey {
                property: "priority".to_string(),
                direction: SortDirection::Asc,
            }]
        );
    }

    #[test]
    fn view_registry() {
        let mut registry = ViewRegistry::new();
        let view = View::new("v1", "View 1");
        registry.set(view.clone());
        assert_eq!(registry.get("v1"), Some(&view));
        assert_eq!(registry.list().len(), 1);
        registry.remove("v1");
        assert!(registry.get("v1").is_none());
    }

    #[test]
    fn template_all_notes() {
        let view = templates::all_notes();
        assert_eq!(view.id, "all-notes");
        assert!(!view.query.sort.is_empty());
    }

    #[test]
    fn template_by_tag() {
        let view = templates::by_tag("rust");
        assert_eq!(view.id, "tag-rust");
        assert_eq!(view.name, "#rust");
    }
}
