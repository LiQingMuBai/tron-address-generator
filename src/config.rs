use std::path::Path;

use roxmltree::{Document, Node};

use crate::error::{AppError, Result};

/// Supported export file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Txt,
    Json,
}

impl ExportFormat {
    /// Parse a case-insensitive format name (`CSV` / `TXT` / `JSON`).
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "CSV" => Ok(Self::Csv),
            "TXT" => Ok(Self::Txt),
            "JSON" => Ok(Self::Json),
            other => Err(AppError::Unsupported {
                field: "ExportFormat".to_string(),
                value: other.to_string(),
            }),
        }
    }

    /// File extension used for the exported artifact.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Txt => "txt",
            Self::Json => "json",
        }
    }

    /// Canonical display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Txt => "TXT",
            Self::Json => "JSON",
        }
    }
}

/// How multiple conditions are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOperator {
    And,
    Or,
}

impl LogicOperator {
    /// Parse a case-insensitive operator name (`AND` / `OR`).
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "AND" => Ok(Self::And),
            "OR" => Ok(Self::Or),
            other => Err(AppError::Unsupported {
                field: "LogicOperator".to_string(),
                value: other.to_string(),
            }),
        }
    }

    /// Canonical display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "AND",
            Self::Or => "OR",
        }
    }
}

/// Where a repeating character run must appear inside the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// The run must terminate the address (trailing run).
    Last,
    /// The run may appear anywhere in the address.
    Anywhere,
}

impl Position {
    /// Parse a case-insensitive position name (`LAST` / `ANYWHERE`).
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "LAST" => Ok(Self::Last),
            "ANYWHERE" => Ok(Self::Anywhere),
            "ANY" | "ALL" => Ok(Self::Anywhere),
            other => Err(AppError::Unsupported {
                field: "Position".to_string(),
                value: other.to_string(),
            }),
        }
    }

    /// Canonical display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Last => "Last",
            Self::Anywhere => "Anywhere",
        }
    }
}

/// A single match condition declared in `<Condition>`.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Requires a run of `min_count` identical `ch` characters.
    Repeating {
        ch: char,
        min_count: usize,
        position: Position,
    },
    /// Requires `ch` at the given 1-based index counted from the end.
    FixedChar { ch: char, index_from_end: usize },
    /// Requires the address to match an arbitrary regular expression.
    Regex { pattern: String },
    /// Requires the address to end with the given literal string.
    Suffix { value: String },
}

/// `<ChainConfig>` node.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub chain_type: String,
    pub address_prefix: String,
    pub address_length: usize,
    pub private_key_format: String,
}

/// `<TaskConfig>` node.
#[derive(Debug, Clone)]
pub struct TaskConfig {
    pub target_count: usize,
    pub thread_count: usize,
    pub export_format: ExportFormat,
}

/// `<CompositeConditions>` node.
#[derive(Debug, Clone)]
pub struct CompositeConditions {
    pub logic: LogicOperator,
    pub conditions: Vec<Condition>,
}

/// Root configuration object.
#[derive(Debug, Clone)]
pub struct Config {
    pub chain: ChainConfig,
    pub task: TaskConfig,
    pub conditions: CompositeConditions,
}

impl Config {
    /// Load and validate the configuration from an XML file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(AppError::ConfigNotFound(path.to_path_buf()));
        }
        let raw = std::fs::read_to_string(path).map_err(|source| AppError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let config = Self::from_xml(&raw)?;
        config.validate()?;
        Ok(config)
    }

    /// Parse a configuration from an in-memory XML document.
    pub fn from_xml(raw: &str) -> Result<Self> {
        let doc = Document::parse(raw).map_err(|e| AppError::ConfigParse(e.to_string()))?;
        let root = doc.root_element();
        if !root.has_tag_name("AddressGeneratorConfig") {
            return Err(AppError::ConfigParse(format!(
                "root element must be <AddressGeneratorConfig>, found <{}>",
                root.tag_name().name()
            )));
        }

        let chain_node = child_element(root, "ChainConfig")
            .ok_or_else(|| AppError::MissingField("ChainConfig".to_string()))?;
        let task_node = child_element(root, "TaskConfig")
            .ok_or_else(|| AppError::MissingField("TaskConfig".to_string()))?;
        let conditions_node = child_element(root, "CompositeConditions")
            .ok_or_else(|| AppError::MissingField("CompositeConditions".to_string()))?;

        let chain = ChainConfig {
            chain_type: required_text(chain_node, "ChainType")?,
            address_prefix: required_text(chain_node, "AddressPrefix")?,
            address_length: parse_usize(
                &required_text(chain_node, "AddressLength")?,
                "AddressLength",
            )?,
            private_key_format: required_text(chain_node, "PrivateKeyFormat")?,
        };

        let thread_count = match child_element(task_node, "ThreadCount") {
            Some(node) => parse_usize(&node_text(node), "ThreadCount")?,
            None => 0,
        };

        let task = TaskConfig {
            target_count: parse_usize(
                &required_text(task_node, "TargetCount")?,
                "TargetCount",
            )?,
            thread_count,
            export_format: ExportFormat::parse(&required_text(task_node, "ExportFormat")?)?,
        };

        let logic = match child_element(conditions_node, "LogicOperator") {
            Some(node) => LogicOperator::parse(&node_text(node))?,
            None => LogicOperator::And,
        };

        let mut conditions = Vec::new();
        for node in
            conditions_node.children().filter(|n| n.is_element() && n.has_tag_name("Condition"))
        {
            conditions.push(parse_condition(node)?);
        }

        Ok(Self {
            chain,
            task,
            conditions: CompositeConditions { logic, conditions },
        })
    }

    /// Reject configurations that this generator cannot honour.
    pub fn validate(&self) -> Result<()> {
        let chain_type = self.chain.chain_type.trim().to_ascii_uppercase();
        if chain_type != "TRON" {
            return Err(AppError::Unsupported {
                field: "ChainType".to_string(),
                value: self.chain.chain_type.clone(),
            });
        }

        let key_format = self.chain.private_key_format.trim().to_ascii_uppercase();
        if key_format != "HEX" {
            return Err(AppError::Unsupported {
                field: "PrivateKeyFormat".to_string(),
                value: self.chain.private_key_format.clone(),
            });
        }

        if self.chain.address_prefix.trim().to_ascii_uppercase() != "T" {
            return Err(AppError::InvalidValue {
                field: "AddressPrefix".to_string(),
                reason: "TRON addresses must start with 'T'".to_string(),
            });
        }

        if self.chain.address_length != 34 {
            return Err(AppError::InvalidValue {
                field: "AddressLength".to_string(),
                reason: format!(
                    "TRON addresses are always 34 characters, got {}",
                    self.chain.address_length
                ),
            });
        }

        if self.task.target_count == 0 {
            return Err(AppError::InvalidValue {
                field: "TargetCount".to_string(),
                reason: "must be greater than zero".to_string(),
            });
        }

        Ok(())
    }
}

/// 1-based index counted from the end is used to address a position inside the string.
fn parse_condition(node: Node<'_, '_>) -> Result<Condition> {
    let type_name = required_text(node, "Type")?;
    let param =
        child_element(node, "Param").ok_or_else(|| AppError::MissingField("Param".to_string()))?;

    match type_name.trim().to_ascii_uppercase().as_str() {
        "REPEATING" => {
            let ch = single_char(&required_text(param, "Char")?, "Char")?;
            let min_count = parse_usize(&required_text(param, "MinCount")?, "MinCount")?;
            let position = match child_element(param, "Position") {
                Some(node) => Position::parse(&node_text(node))?,
                None => Position::Last,
            };
            Ok(Condition::Repeating {
                ch,
                min_count,
                position,
            })
        }
        "FIXEDCHAR" | "FIXED" => {
            let ch = single_char(&required_text(param, "Char")?, "Char")?;
            let index_from_end =
                parse_usize(&required_text(param, "IndexFromEnd")?, "IndexFromEnd")?;
            Ok(Condition::FixedChar { ch, index_from_end })
        }
        "REGEX" => Ok(Condition::Regex {
            pattern: required_text(param, "Pattern")?,
        }),
        "SUFFIX" | "ENDSWITH" | "ENDS_WITH" => Ok(Condition::Suffix {
            value: required_text(param, "Value")?,
        }),
        other => Err(AppError::Unsupported {
            field: "Condition/Type".to_string(),
            value: other.to_string(),
        }),
    }
}

/// First element child matching a tag name.
fn child_element<'a>(node: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    node.children()
        .find(|n| n.is_element() && n.has_tag_name(tag))
}

/// Concatenated text content of an element, trimmed.
fn node_text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Text content of a mandatory child element.
fn required_text(node: Node<'_, '_>, tag: &str) -> Result<String> {
    let child =
        child_element(node, tag).ok_or_else(|| AppError::MissingField(tag.to_string()))?;
    let text = node_text(child);
    if text.is_empty() {
        return Err(AppError::MissingField(tag.to_string()));
    }
    Ok(text)
}

/// Parse a base-10 unsigned integer with a descriptive error message.
fn parse_usize(value: &str, field: &str) -> Result<usize> {
    value.trim().parse::<usize>().map_err(|e| AppError::InvalidValue {
        field: field.to_string(),
        reason: e.to_string(),
    })
}

/// Extract exactly one ASCII character from a trimmed string.
pub(crate) fn single_char(value: &str, field: &str) -> Result<char> {
    let mut chars = value.trim().chars();
    let first = chars.next().ok_or_else(|| AppError::InvalidValue {
        field: field.to_string(),
        reason: "must not be empty".to_string(),
    })?;
    if chars.next().is_some() {
        return Err(AppError::InvalidValue {
            field: field.to_string(),
            reason: "must be exactly one character".to_string(),
        });
    }
    if !first.is_ascii() {
        return Err(AppError::InvalidValue {
            field: field.to_string(),
            reason: "must be an ASCII character".to_string(),
        });
    }
    Ok(first)
}
