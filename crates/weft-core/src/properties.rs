//! Typed property schema, inference, coercion, and validation.
//!
//! Properties are derived from frontmatter and drive views, tasks, and canvas.
//! The schema is inferred from observed values and can be overridden in
//! `vault.toml`. Coercion is tolerant: bad values never crash a view.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use toml::Value as TomlValue;
use toml::value::Datetime as TomlDatetime;

/// Property values keyed by property name.
pub type PropertyMap = HashMap<String, PropertyValue>;

/// Supported property types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PropertyType {
    /// Free-form text.
    Text,
    /// Integer or floating-point number.
    Number,
    /// Date (with optional time and timezone).
    Date,
    /// Boolean value.
    Bool,
    /// Single choice from a fixed set of options.
    Select,
    /// Multiple choices from a fixed set of options.
    MultiSelect,
    /// Link to another note (`[[note]]`).
    Link,
    /// Tags (`#tag`).
    Tags,
}

impl PropertyType {
    /// All known types, in a stable order for UI.
    pub fn all() -> &'static [Self] {
        &[
            Self::Text,
            Self::Number,
            Self::Date,
            Self::Bool,
            Self::Select,
            Self::MultiSelect,
            Self::Link,
            Self::Tags,
        ]
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Date => "Date",
            Self::Bool => "Checkbox",
            Self::Select => "Select",
            Self::MultiSelect => "Multi-select",
            Self::Link => "Link",
            Self::Tags => "Tags",
        }
    }

    /// Whether this type can hold multiple values.
    #[must_use]
    pub fn is_multi(self) -> bool {
        matches!(self, Self::MultiSelect | Self::Tags)
    }

    /// Default empty value for this type.
    #[must_use]
    pub fn default_value(self) -> PropertyValue {
        match self {
            Self::Text => PropertyValue::Text(String::new()),
            Self::Number => PropertyValue::Number(0.0),
            Self::Date => PropertyValue::Date(None),
            Self::Bool => PropertyValue::Bool(false),
            Self::Select => PropertyValue::Select(String::new()),
            Self::MultiSelect => PropertyValue::MultiSelect(Vec::new()),
            Self::Link => PropertyValue::Link(String::new()),
            Self::Tags => PropertyValue::Tags(Vec::new()),
        }
    }
}

/// Serialize `Option<DateTime<FixedOffset>>` as an RFC3339 string.
#[expect(
    clippy::ref_option,
    reason = "serde's serialize_with requires a reference to the field type"
)]
fn serialize_datetime<S>(
    dt: &Option<DateTime<FixedOffset>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match dt {
        Some(d) => serializer.serialize_str(&d.to_rfc3339()),
        None => serializer.serialize_none(),
    }
}

/// Deserialize `Option<DateTime<FixedOffset>>` from an RFC3339 string.
fn deserialize_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime<FixedOffset>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct DateTimeVisitor;

    impl<'de> Visitor<'de> for DateTimeVisitor {
        type Value = Option<DateTime<FixedOffset>>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an RFC3339 datetime string or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: Deserializer<'de>,
        {
            deserializer.deserialize_str(self)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            DateTime::parse_from_rfc3339(value)
                .map(Some)
                .map_err(|error| de::Error::custom(format!("invalid datetime: {error}")))
        }
    }

    deserializer.deserialize_option(DateTimeVisitor)
}

/// A property value with its inferred or declared type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    #[serde(
        serialize_with = "serialize_datetime",
        deserialize_with = "deserialize_datetime"
    )]
    Date(Option<DateTime<FixedOffset>>),
    Bool(bool),
    Select(String),
    MultiSelect(Vec<String>),
    Link(String),
    Tags(Vec<String>),
}

impl PropertyValue {
    /// The type of this value.
    #[must_use]
    pub fn prop_type(&self) -> PropertyType {
        match self {
            Self::Text(_) => PropertyType::Text,
            Self::Number(_) => PropertyType::Number,
            Self::Date(_) => PropertyType::Date,
            Self::Bool(_) => PropertyType::Bool,
            Self::Select(_) => PropertyType::Select,
            Self::MultiSelect(_) => PropertyType::MultiSelect,
            Self::Link(_) => PropertyType::Link,
            Self::Tags(_) => PropertyType::Tags,
        }
    }

    /// Convert to a string for display.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Text(s) | Self::Select(s) | Self::Link(s) => s.clone(),
            Self::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{n:.0}")
                } else {
                    n.to_string()
                }
            }
            Self::Date(Some(dt)) => dt.format("%Y-%m-%d").to_string(),
            Self::Date(None) => String::new(),
            Self::Bool(b) => b.to_string(),
            Self::MultiSelect(v) => v.join(", "),
            Self::Tags(v) => v
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// Convert to a sortable key (for ordering in views).
    #[must_use]
    pub fn sort_key(&self) -> String {
        match self {
            Self::Text(s) | Self::Select(s) | Self::Link(s) => s.to_lowercase(),
            Self::Number(n) => format!("{n:020.6}"),
            Self::Date(Some(dt)) => dt.format("%Y-%m-%d").to_string(),
            Self::Date(None) => String::new(),
            Self::Bool(b) => if *b { "1" } else { "0" }.to_string(),
            Self::MultiSelect(v) | Self::Tags(v) => v.join(",").to_lowercase(),
        }
    }

    /// Whether this value is "empty" for filtering purposes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(s) | Self::Select(s) | Self::Link(s) => s.trim().is_empty(),
            Self::Number(n) => n.is_nan(),
            Self::Date(d) => d.is_none(),
            Self::Bool(b) => !*b,
            Self::MultiSelect(v) | Self::Tags(v) => v.is_empty(),
        }
    }

    /// Convert to a `toml::Value` for serialization to frontmatter.
    #[must_use]
    pub fn to_toml_value(&self) -> TomlValue {
        match self {
            Self::Text(s) | Self::Select(s) | Self::Link(s) => TomlValue::String(s.clone()),
            Self::Number(n) => number_to_toml(*n),
            Self::Date(Some(dt)) => TomlValue::String(dt.to_rfc3339()),
            Self::Date(None) => TomlValue::String(String::new()),
            Self::Bool(b) => TomlValue::Boolean(*b),
            Self::MultiSelect(v) | Self::Tags(v) => {
                TomlValue::Array(v.iter().cloned().map(TomlValue::String).collect())
            }
        }
    }
}

impl std::str::FromStr for PropertyValue {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Try number
        if let Ok(n) = s.parse::<f64>() {
            return Ok(Self::Number(n));
        }
        // Try bool
        if let Ok(b) = s.parse::<bool>() {
            return Ok(Self::Bool(b));
        }
        // Try date (RFC3339, then YYYY-MM-DD)
        if let Some(dt) = date_from_str(s) {
            return Ok(Self::Date(Some(dt)));
        }
        // Default to text
        Ok(Self::Text(s.to_string()))
    }
}

/// A property definition: name, type, and optional constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PropertyDef {
    /// Property name (as it appears in frontmatter).
    pub name: String,
    /// Type of this property.
    pub prop_type: PropertyType,
    /// For select/multi-select: allowed options.
    pub options: Vec<String>,
    /// Whether this property is required (shown in UI but not enforced).
    pub required: bool,
    /// Default value for new notes.
    pub default: Option<PropertyValue>,
}

impl Default for PropertyDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            prop_type: PropertyType::Text,
            options: Vec::new(),
            required: false,
            default: None,
        }
    }
}

/// Complete property schema for a vault.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PropertySchema {
    /// Property definitions by name.
    pub properties: HashMap<String, PropertyDef>,
    /// Property name aliases (e.g., "tags" -> "categories").
    pub aliases: HashMap<String, String>,
}

impl PropertySchema {
    /// Create an empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a property definition by name (following aliases).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PropertyDef> {
        let canonical = self.canonical_name(name);
        self.properties.get(&canonical)
    }

    /// Get the canonical name for a property (resolving aliases).
    #[must_use]
    pub fn canonical_name(&self, name: &str) -> String {
        self.aliases
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// All property names in a stable order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.properties.keys().cloned().collect();
        names.sort();
        names
    }

    /// Add or update a property definition.
    pub fn set(&mut self, def: PropertyDef) {
        self.properties.insert(def.name.clone(), def);
    }

    /// Remove a property definition.
    pub fn remove(&mut self, name: &str) -> Option<PropertyDef> {
        let canonical = self.canonical_name(name);
        self.aliases.retain(|_, v| *v != canonical);
        self.properties.remove(&canonical)
    }

    /// Add an alias for a property.
    pub fn add_alias(&mut self, alias: String, target: String) {
        if self.properties.contains_key(&target) {
            self.aliases.insert(alias, target);
        }
    }
}

/// Inferred property value from a raw TOML value.
#[derive(Debug, Clone)]
struct InferredValue {
    // Only the type is used in production; tests check the value.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the inference tests"))]
    value: PropertyValue,
    prop_type: PropertyType,
}

/// Infer a property's type and value from a raw TOML value.
fn infer_from_toml(value: &TomlValue) -> InferredValue {
    match value {
        TomlValue::String(s) => {
            let trimmed = s.trim();
            if let Some(dt) = date_from_str(trimmed) {
                return InferredValue {
                    value: PropertyValue::Date(Some(dt)),
                    prop_type: PropertyType::Date,
                };
            }
            // Check for wikilink-like pattern
            if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
                return InferredValue {
                    value: PropertyValue::Link(trimmed[2..trimmed.len() - 2].to_string()),
                    prop_type: PropertyType::Link,
                };
            }
            // Default to text
            InferredValue {
                value: PropertyValue::Text(s.clone()),
                prop_type: PropertyType::Text,
            }
        }
        TomlValue::Integer(i) => InferredValue {
            value: PropertyValue::Number(*i as f64),
            prop_type: PropertyType::Number,
        },
        TomlValue::Float(f) => InferredValue {
            value: PropertyValue::Number(*f),
            prop_type: PropertyType::Number,
        },
        TomlValue::Boolean(b) => InferredValue {
            value: PropertyValue::Bool(*b),
            prop_type: PropertyType::Bool,
        },
        TomlValue::Array(arr) => {
            // Check if all elements are strings
            let strings: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect();
            if strings.len() == arr.len() {
                // Could be tags or multi-select
                let has_hash = strings.iter().any(|s| s.starts_with('#'));
                InferredValue {
                    value: if has_hash {
                        PropertyValue::Tags(
                            strings
                                .iter()
                                .map(|s| s.trim_start_matches('#').to_string())
                                .collect(),
                        )
                    } else {
                        PropertyValue::MultiSelect(strings)
                    },
                    prop_type: if has_hash {
                        PropertyType::Tags
                    } else {
                        PropertyType::MultiSelect
                    },
                }
            } else {
                InferredValue {
                    value: PropertyValue::Text(value.to_string()),
                    prop_type: PropertyType::Text,
                }
            }
        }
        TomlValue::Datetime(dt) => InferredValue {
            value: datetime_from_toml(dt),
            prop_type: PropertyType::Date,
        },
        TomlValue::Table(_) => InferredValue {
            value: PropertyValue::Text(value.to_string()),
            prop_type: PropertyType::Text,
        },
    }
}

/// Coerce a raw TOML value into a `PropertyValue` of the given type.
///
/// Returns the default for the type if coercion fails.
#[must_use]
pub fn coerce_value(value: &TomlValue, target_type: PropertyType) -> PropertyValue {
    match target_type {
        PropertyType::Text => match value {
            TomlValue::String(s) => PropertyValue::Text(s.clone()),
            _ => PropertyValue::Text(value.to_string()),
        },
        PropertyType::Number => match value {
            TomlValue::Integer(i) => PropertyValue::Number(*i as f64),
            TomlValue::Float(f) => PropertyValue::Number(*f),
            TomlValue::String(s) => PropertyValue::Number(s.trim().parse().unwrap_or(0.0)),
            _ => PropertyValue::Number(0.0),
        },
        PropertyType::Date => match value {
            TomlValue::Datetime(dt) => datetime_from_toml(dt),
            TomlValue::String(s) => date_from_str(s).map_or(PropertyValue::Date(None), |dt| {
                PropertyValue::Date(Some(dt))
            }),
            _ => PropertyValue::Date(None),
        },
        PropertyType::Bool => match value {
            TomlValue::Boolean(b) => PropertyValue::Bool(*b),
            TomlValue::String(s) => PropertyValue::Bool(s.trim().parse().unwrap_or(false)),
            TomlValue::Integer(i) => PropertyValue::Bool(*i != 0),
            _ => PropertyValue::Bool(false),
        },
        PropertyType::Select => match value {
            TomlValue::String(s) => PropertyValue::Select(s.clone()),
            _ => PropertyValue::Select(value.to_string()),
        },
        PropertyType::MultiSelect => match value {
            TomlValue::Array(arr) => {
                let strings: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect();
                PropertyValue::MultiSelect(strings)
            }
            TomlValue::String(s) => PropertyValue::MultiSelect(split_list(s)),
            _ => PropertyValue::MultiSelect(Vec::new()),
        },
        PropertyType::Link => match value {
            TomlValue::String(s) => PropertyValue::Link(
                s.trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string(),
            ),
            _ => PropertyValue::Link(value.to_string()),
        },
        PropertyType::Tags => match value {
            TomlValue::Array(arr) => {
                let strings: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim_start_matches('#').to_string()))
                    .collect();
                PropertyValue::Tags(strings)
            }
            TomlValue::String(s) => PropertyValue::Tags(
                split_list(s)
                    .into_iter()
                    .map(|s| s.trim_start_matches('#').to_string())
                    .collect(),
            ),
            _ => PropertyValue::Tags(Vec::new()),
        },
    }
}

/// Extract properties from a frontmatter TOML table.
#[must_use]
pub fn extract_properties(frontmatter: &TomlValue, schema: &PropertySchema) -> PropertyMap {
    let TomlValue::Table(table) = frontmatter else {
        return PropertyMap::new();
    };

    let mut result = PropertyMap::new();
    for (key, value) in table {
        // Skip internal keys
        if key == "title" || key == "aliases" {
            continue;
        }
        let canonical = schema.canonical_name(key);
        let prop_type = schema.get(&canonical).map_or_else(
            || infer_from_toml(value).prop_type,
            |definition| definition.prop_type,
        );
        result.insert(canonical, coerce_value(value, prop_type));
    }
    result
}

/// Build a schema by inferring from a collection of notes.
#[must_use]
pub fn infer_schema(
    notes: &[(PathBuf, PropertyMap)],
    existing: Option<&PropertySchema>,
) -> PropertySchema {
    let mut schema = existing.cloned().unwrap_or_default();
    let mut type_votes: HashMap<String, HashMap<PropertyType, usize>> = HashMap::new();

    for (_, props) in notes {
        for (name, value) in props {
            let entry = type_votes.entry(name.clone()).or_default();
            *entry.entry(value.prop_type()).or_default() += 1;
        }
    }

    for (name, votes) in type_votes {
        let (best_type, _) = votes
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .unwrap_or((PropertyType::Text, 0));
        let canonical = schema.canonical_name(&name);
        if !schema.properties.contains_key(&canonical) {
            schema.set(PropertyDef {
                name: canonical,
                prop_type: best_type,
                options: Vec::new(),
                required: false,
                default: None,
            });
        }
    }

    schema
}

/// Load property overrides from `vault.toml`.
pub fn load_overrides(vault_toml: &str) -> Result<PropertySchema> {
    let mut schema = PropertySchema::new();
    let value: TomlValue = vault_toml.parse()?;
    if let Some(props) = value.get("properties").and_then(TomlValue::as_table) {
        for (name, def_value) in props {
            if let TomlValue::Table(table) = def_value {
                let prop_type = table
                    .get("type")
                    .and_then(TomlValue::as_str)
                    .and_then(parse_property_type)
                    .unwrap_or(PropertyType::Text);

                let options = table
                    .get("options")
                    .and_then(TomlValue::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default();

                let required = table
                    .get("required")
                    .and_then(TomlValue::as_bool)
                    .unwrap_or(false);

                let default = table.get("default").map(|v| coerce_value(v, prop_type));

                schema.set(PropertyDef {
                    name: name.clone(),
                    prop_type,
                    options,
                    required,
                    default,
                });
            }
        }
    }
    Ok(schema)
}

/// Parse a property-type name from `vault.toml`.
fn parse_property_type(name: &str) -> Option<PropertyType> {
    match name {
        "text" => Some(PropertyType::Text),
        "number" => Some(PropertyType::Number),
        "date" => Some(PropertyType::Date),
        "bool" | "checkbox" => Some(PropertyType::Bool),
        "select" => Some(PropertyType::Select),
        "multi-select" => Some(PropertyType::MultiSelect),
        "link" => Some(PropertyType::Link),
        "tags" => Some(PropertyType::Tags),
        _ => None,
    }
}

/// Convert a `Frontmatter` struct to a map of property values.
///
/// Uses the schema for type inference when available, otherwise infers from
/// the values themselves.
#[must_use]
pub fn frontmatter_to_properties(
    fm: &crate::document::Frontmatter,
    schema: Option<&PropertySchema>,
) -> PropertyMap {
    let mut props = PropertyMap::new();

    if let Some(title) = &fm.title {
        props.insert("title".to_string(), PropertyValue::Text(title.clone()));
    }

    if !fm.tags.is_empty() {
        props.insert("tags".to_string(), PropertyValue::Tags(fm.tags.clone()));
    }

    if let Some(dt) = fm.created.as_deref().and_then(parse_date_string) {
        props.insert("created".to_string(), PropertyValue::Date(Some(dt)));
    }

    if let Some(dt) = fm.modified.as_deref().and_then(parse_date_string) {
        props.insert("modified".to_string(), PropertyValue::Date(Some(dt)));
    }

    for (key, value) in &fm.other {
        // Use the schema when it has a type for this key.
        let prop_type = schema.and_then(|s| s.get(key)).map(|d| d.prop_type);

        if let Some(pv) = toml_value_to_property(value, prop_type) {
            props.insert(key.clone(), pv);
        }
    }

    props
}

/// Convert a `toml` datetime to a property date, assuming UTC without an offset.
fn datetime_from_toml(dt: &TomlDatetime) -> PropertyValue {
    let Some(day) = dt.date.and_then(|d| {
        NaiveDate::from_ymd_opt(i32::from(d.year), u32::from(d.month), u32::from(d.day))
    }) else {
        return PropertyValue::Date(None);
    };

    let naive = match dt.time {
        Some(t) => day.and_hms_nano_opt(
            u32::from(t.hour),
            u32::from(t.minute),
            u32::from(t.second),
            t.nanosecond,
        ),
        None => Some(day.and_time(NaiveTime::MIN)),
    };
    let Some(naive) = naive else {
        return PropertyValue::Date(None);
    };

    let offset = match dt.offset {
        Some(toml::value::Offset::Custom { minutes }) => {
            FixedOffset::east_opt(i32::from(minutes) * 60)
        }
        _ => FixedOffset::east_opt(0),
    };
    let Some(offset) = offset else {
        return PropertyValue::Date(None);
    };

    PropertyValue::Date(Some(DateTime::from_naive_utc_and_offset(naive, offset)))
}

/// Parse an RFC3339 or `YYYY-MM-DD` string as UTC when no offset is given.
fn date_from_str(s: &str) -> Option<DateTime<FixedOffset>> {
    let trimmed = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt);
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .ok()
        .map(|date| date.and_time(NaiveTime::MIN).and_utc().fixed_offset())
}

/// Parse the date formats accepted in frontmatter.
fn parse_date_string(s: &str) -> Option<DateTime<FixedOffset>> {
    if let Some(dt) = date_from_str(s) {
        return Some(dt);
    }
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc().fixed_offset())
}

/// Convert a number to an integer TOML value when it has no fraction.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is range-checked before the cast"
)]
fn number_to_toml(n: f64) -> TomlValue {
    if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        TomlValue::Integer(n as i64)
    } else {
        TomlValue::Float(n)
    }
}

/// Split a comma-separated property into trimmed, non-empty items.
fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Coerce one frontmatter value to a property value, inferring the type if needed.
fn toml_value_to_property(
    value: &TomlValue,
    prop_type: Option<PropertyType>,
) -> Option<PropertyValue> {
    // Infer a type from the TOML value when the schema has none.
    let inferred_type = prop_type.unwrap_or_else(|| match value {
        TomlValue::Integer(_) | TomlValue::Float(_) => PropertyType::Number,
        TomlValue::Boolean(_) => PropertyType::Bool,
        TomlValue::String(_) | TomlValue::Table(_) => PropertyType::Text,
        TomlValue::Array(arr) => {
            if arr.is_empty() {
                PropertyType::Text
            } else if arr.iter().all(|v| matches!(v, TomlValue::String(_))) {
                // Check if it looks like tags (no spaces, lowercase)
                if arr.iter().all(|v| {
                    v.as_str()
                        .is_some_and(|s| !s.contains(' ') && s == s.to_lowercase())
                }) {
                    PropertyType::Tags
                } else {
                    PropertyType::MultiSelect
                }
            } else {
                PropertyType::MultiSelect
            }
        }
        TomlValue::Datetime(_) => PropertyType::Date,
    });

    // Coerce the value to the determined type.
    match inferred_type {
        PropertyType::Text => value.as_str().map(|s| PropertyValue::Text(s.to_string())),
        PropertyType::Number => match value {
            TomlValue::Integer(i) => Some(PropertyValue::Number(*i as f64)),
            TomlValue::Float(f) => Some(PropertyValue::Number(*f)),
            TomlValue::String(s) => s.parse::<f64>().ok().map(PropertyValue::Number),
            _ => None,
        },
        PropertyType::Date => match value {
            TomlValue::Datetime(dt) => Some(datetime_from_toml(dt)),
            TomlValue::String(s) => date_from_str(s).map(|dt| PropertyValue::Date(Some(dt))),
            _ => None,
        },
        PropertyType::Bool => value.as_bool().map(PropertyValue::Bool),
        PropertyType::Select => value.as_str().map(|s| PropertyValue::Select(s.to_string())),
        PropertyType::MultiSelect => match value {
            TomlValue::Array(arr) => Some(PropertyValue::MultiSelect(
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect(),
            )),
            TomlValue::String(s) => Some(PropertyValue::MultiSelect(split_list(s))),
            _ => None,
        },
        PropertyType::Link => value.as_str().map(|s| PropertyValue::Link(s.to_string())),
        PropertyType::Tags => match value {
            TomlValue::Array(arr) => Some(PropertyValue::Tags(
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect(),
            )),
            TomlValue::String(s) => Some(PropertyValue::Tags(split_list(s))),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_string_is_text() {
        let v = TomlValue::String("hello".to_string());
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Text);
        assert_eq!(inferred.value, PropertyValue::Text("hello".to_string()));
    }

    #[test]
    fn infer_integer_is_number() {
        let v = TomlValue::Integer(42);
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Number);
        assert_eq!(inferred.value, PropertyValue::Number(42.0));
    }

    #[test]
    fn infer_float_is_number() {
        let v = TomlValue::Float(3.5);
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Number);
        assert_eq!(inferred.value, PropertyValue::Number(3.5));
    }

    #[test]
    fn infer_boolean_is_bool() {
        let v = TomlValue::Boolean(true);
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Bool);
        assert_eq!(inferred.value, PropertyValue::Bool(true));
    }

    #[test]
    fn infer_rfc3339_date() {
        let v = TomlValue::String("2024-01-15T10:30:00Z".to_string());
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Date);
        assert!(matches!(inferred.value, PropertyValue::Date(Some(_))));
    }

    #[test]
    fn infer_naive_date() {
        let v = TomlValue::String("2024-01-15".to_string());
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Date);
    }

    #[test]
    fn infer_wikilink_as_link() {
        let v = TomlValue::String("[[note]]".to_string());
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Link);
    }

    #[test]
    fn infer_array_of_strings_as_multiselect() {
        let v = TomlValue::Array(vec![
            TomlValue::String("a".to_string()),
            TomlValue::String("b".to_string()),
        ]);
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::MultiSelect);
        assert_eq!(
            inferred.value,
            PropertyValue::MultiSelect(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn infer_array_with_hash_as_tags() {
        let v = TomlValue::Array(vec![
            TomlValue::String("#rust".to_string()),
            TomlValue::String("#notes".to_string()),
        ]);
        let inferred = infer_from_toml(&v);
        assert_eq!(inferred.prop_type, PropertyType::Tags);
        assert_eq!(
            inferred.value,
            PropertyValue::Tags(vec!["rust".to_string(), "notes".to_string()])
        );
    }

    #[test]
    fn coerce_number_from_string() {
        let v = TomlValue::String("42".to_string());
        let coerced = coerce_value(&v, PropertyType::Number);
        assert_eq!(coerced, PropertyValue::Number(42.0));
    }

    #[test]
    fn coerce_bool_from_string() {
        let v = TomlValue::String("true".to_string());
        let coerced = coerce_value(&v, PropertyType::Bool);
        assert_eq!(coerced, PropertyValue::Bool(true));
    }

    #[test]
    fn coerce_date_from_string() {
        let v = TomlValue::String("2024-01-15".to_string());
        let coerced = coerce_value(&v, PropertyType::Date);
        assert!(matches!(coerced, PropertyValue::Date(Some(_))));
    }

    #[test]
    fn coerce_multiselect_from_string() {
        let v = TomlValue::String("a, b, c".to_string());
        let coerced = coerce_value(&v, PropertyType::MultiSelect);
        assert_eq!(
            coerced,
            PropertyValue::MultiSelect(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn coerce_tags_from_string() {
        let v = TomlValue::String("#rust, #notes".to_string());
        let coerced = coerce_value(&v, PropertyType::Tags);
        assert_eq!(
            coerced,
            PropertyValue::Tags(vec!["rust".to_string(), "notes".to_string()])
        );
    }

    #[test]
    fn extract_properties_skips_title_and_aliases() {
        let fm = toml::from_str::<TomlValue>(
            r#"
            title = "My Note"
            aliases = ["note", "memo"]
            status = "done"
            priority = 1
        "#,
        )
        .unwrap();
        let schema = PropertySchema::new();
        let props = extract_properties(&fm, &schema);
        assert!(!props.contains_key("title"));
        assert!(!props.contains_key("aliases"));
        assert!(props.contains_key("status"));
        assert!(props.contains_key("priority"));
    }

    #[test]
    fn schema_alias_resolution() {
        let mut schema = PropertySchema::new();
        schema.set(PropertyDef {
            name: "status".to_string(),
            prop_type: PropertyType::Select,
            ..Default::default()
        });
        schema.add_alias("state".to_string(), "status".to_string());
        assert_eq!(schema.canonical_name("state"), "status".to_string());
        assert!(schema.get("state").is_some());
    }

    #[test]
    fn property_value_is_empty() {
        assert!(PropertyValue::Text(String::new()).is_empty());
        assert!(PropertyValue::Text("   ".to_string()).is_empty());
        assert!(!PropertyValue::Text("x".to_string()).is_empty());
        assert!(PropertyValue::Number(f64::NAN).is_empty());
        assert!(!PropertyValue::Number(0.0).is_empty());
        assert!(PropertyValue::Date(None).is_empty());
        assert!(PropertyValue::Bool(false).is_empty());
        assert!(!PropertyValue::Bool(true).is_empty());
        assert!(PropertyValue::Select(String::new()).is_empty());
        assert!(PropertyValue::MultiSelect(vec![]).is_empty());
        assert!(PropertyValue::Link(String::new()).is_empty());
        assert!(PropertyValue::Tags(vec![]).is_empty());
    }

    #[test]
    fn property_value_sort_key() {
        assert_eq!(
            PropertyValue::Text("Banana".to_string()).sort_key(),
            "banana"
        );
        // {:020.6} = 20 total width, 6 decimal places. 42.0 = "42.000000" (9 chars) + 11 leading zeros
        assert_eq!(
            PropertyValue::Number(42.0).sort_key(),
            "0000000000042.000000"
        );
        assert_eq!(PropertyValue::Bool(true).sort_key(), "1");
        assert_eq!(PropertyValue::Bool(false).sort_key(), "0");
    }

    #[test]
    fn load_overrides_from_vault_toml() {
        let toml = r#"
            [properties.status]
            type = "select"
            options = ["todo", "in-progress", "done"]
            required = true
            default = "todo"
        "#;
        let schema = load_overrides(toml).unwrap();
        let def = schema.get("status").unwrap();
        assert_eq!(def.prop_type, PropertyType::Select);
        assert_eq!(def.options, vec!["todo", "in-progress", "done"]);
        assert!(def.required);
        assert_eq!(def.default, Some(PropertyValue::Select("todo".to_string())));
    }
}
