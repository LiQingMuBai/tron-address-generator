use regex::Regex;

use crate::config::{CompositeConditions, Condition, LogicOperator, Position};
use crate::error::{AppError, Result};

/// A condition compiled for fast repeated evaluation inside the search loop.
#[derive(Debug)]
enum Compiled {
    Repeating {
        ch: u8,
        min_count: usize,
        position: Position,
    },
    FixedChar {
        ch: u8,
        index_from_end: usize,
    },
    Regex(Regex),
    Suffix(Vec<u8>),
}

/// Evaluates all configured conditions against a candidate address.
#[derive(Debug)]
pub struct Matcher {
    logic: LogicOperator,
    conditions: Vec<Compiled>,
}

impl Matcher {
    /// Compile the declared composite conditions, surfacing invalid input early.
    pub fn new(config: &CompositeConditions) -> Result<Self> {
        let mut compiled = Vec::with_capacity(config.conditions.len());

        for condition in &config.conditions {
            let item = match condition {
                Condition::Repeating {
                    ch,
                    min_count,
                    position,
                } => {
                    if *min_count == 0 {
                        return Err(AppError::InvalidValue {
                            field: "MinCount".to_string(),
                            reason: "must be greater than zero".to_string(),
                        });
                    }
                    Compiled::Repeating {
                        ch: *ch as u8,
                        min_count: *min_count,
                        position: *position,
                    }
                }
                Condition::FixedChar { ch, index_from_end } => {
                    if *index_from_end == 0 {
                        return Err(AppError::InvalidValue {
                            field: "IndexFromEnd".to_string(),
                            reason: "must be greater than zero (1 = last character)".to_string(),
                        });
                    }
                    Compiled::FixedChar {
                        ch: *ch as u8,
                        index_from_end: *index_from_end,
                    }
                }
                Condition::Regex { pattern } => {
                    let regex = Regex::new(pattern).map_err(|e| AppError::InvalidValue {
                        field: "Pattern".to_string(),
                        reason: e.to_string(),
                    })?;
                    Compiled::Regex(regex)
                }
                Condition::Suffix { value } => {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        return Err(AppError::InvalidValue {
                            field: "Suffix".to_string(),
                            reason: "must not be empty".to_string(),
                        });
                    }
                    Compiled::Suffix(trimmed.as_bytes().to_vec())
                }
            };
            compiled.push(item);
        }

        Ok(Self {
            logic: config.logic,
            conditions: compiled,
        })
    }

    /// Returns `true` when the address satisfies the composite condition set.
    pub fn matches(&self, address: &str) -> bool {
        if self.conditions.is_empty() {
            // Without any condition every address qualifies.
            return true;
        }
        match self.logic {
            LogicOperator::And => self.conditions.iter().all(|c| check(c, address)),
            LogicOperator::Or => self.conditions.iter().any(|c| check(c, address)),
        }
    }

    /// Human-readable description of every condition, used for logging.
    pub fn describe(&self) -> Vec<String> {
        self.conditions
            .iter()
            .map(|c| match c {
                Compiled::Repeating {
                    ch,
                    min_count,
                    position,
                } => format!(
                    "Repeating '{}' x{} at {}",
                    *ch as char,
                    min_count,
                    position.as_str()
                ),
                Compiled::FixedChar { ch, index_from_end } => {
                    format!("FixedChar '{}' at index {} from end", *ch as char, index_from_end)
                }
                Compiled::Regex(re) => format!("Regex /{}/", re.as_str()),
                Compiled::Suffix(suffix) => {
                    format!("Suffix \"{}\"", String::from_utf8_lossy(suffix))
                }
            })
            .collect()
    }
}

/// Evaluate one compiled condition.
fn check(condition: &Compiled, address: &str) -> bool {
    let bytes = address.as_bytes();
    match condition {
        Compiled::Repeating {
            ch,
            min_count,
            position,
        } => match position {
            Position::Last => {
                let run = bytes.iter().rev().take_while(|b| *b == ch).count();
                run >= *min_count
            }
            Position::Anywhere => {
                let mut run = 0usize;
                for byte in bytes {
                    if byte == ch {
                        run += 1;
                        if run >= *min_count {
                            return true;
                        }
                    } else {
                        run = 0;
                    }
                }
                false
            }
        },
        Compiled::FixedChar { ch, index_from_end } => {
            if *index_from_end == 0 || *index_from_end > bytes.len() {
                return false;
            }
            bytes[bytes.len() - *index_from_end] == *ch
        }
        Compiled::Regex(re) => re.is_match(address),
        Compiled::Suffix(suffix) => bytes.ends_with(suffix),
    }
}
