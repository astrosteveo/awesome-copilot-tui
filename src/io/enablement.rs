use anyhow::{Context, Result};
use chrono::Utc;
use jsonschema::{paths::JSONPointer, JSONSchema, ValidationError};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde_json::Value;
use std::{fmt, fs, io::Write};

use crate::domain::model::EnablementFile;

use super::paths::RepoPaths;

const SCHEMA_JSON: &str = include_str!("../../docs/schemas/enablement.schema.json");

#[derive(Debug, Clone, Deserialize)]
pub enum EnablementWarning {
    MissingFile,
    ParseError(String),
    SchemaValidation(Vec<String>),
}

impl fmt::Display for EnablementWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnablementWarning::MissingFile => {
                write!(
                    f,
                    "No enablement file found; starting from a disabled baseline."
                )
            }
            EnablementWarning::ParseError(err) => {
                write!(f, "Failed to parse enablement file: {err}")
            }
            EnablementWarning::SchemaValidation(errors) => {
                write!(
                    f,
                    "Enablement file failed schema validation: {}",
                    errors.join(", ")
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnablementLoad {
    pub file: EnablementFile,
    pub warnings: Vec<EnablementWarning>,
}

pub fn load_enablement(paths: &RepoPaths) -> Result<EnablementLoad> {
    match fs::read_to_string(&paths.enablement) {
        Ok(content) => parse_enablement(&content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(EnablementLoad {
            file: EnablementFile::default(),
            warnings: vec![EnablementWarning::MissingFile],
        }),
        Err(err) => Err(err).with_context(|| {
            format!(
                "Failed to read enablement file from {}",
                paths.enablement.display()
            )
        }),
    }
}

fn parse_enablement(content: &str) -> Result<EnablementLoad> {
    let value: Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(err) => {
            return Ok(EnablementLoad {
                file: EnablementFile::default(),
                warnings: vec![EnablementWarning::ParseError(err.to_string())],
            })
        }
    };

    let schema = schema();
    let mut validation_errors = Vec::new();
    if let Err(errors) = schema.validate(&value) {
        for error in errors.into_iter() {
            let path = format_pointer(&error.instance_path);
            validation_errors.push(format!("{}: {}", path, error));
        }
    }

    if !validation_errors.is_empty() {
        return Ok(EnablementLoad {
            file: EnablementFile::default(),
            warnings: vec![EnablementWarning::SchemaValidation(validation_errors)],
        });
    }

    let mut file: EnablementFile = serde_json::from_value(value)
        .context("Failed to deserialize enablement file into struct")?;
    if file.version == 0 {
        file.version = 1;
    }

    Ok(EnablementLoad {
        file,
        warnings: Vec::new(),
    })
}

pub fn save_enablement(paths: &RepoPaths, file: &mut EnablementFile) -> Result<()> {
    file.updated_at = Some(Utc::now());
    let value = serde_json::to_value(&file).context("Failed to serialize enablement file")?;
    let schema = schema();
    if let Err(errors) = schema.validate(&value) {
        return Err(anyhow::anyhow!(format_validation_errors(
            errors.into_iter()
        )));
    }

    let json =
        serde_json::to_string_pretty(&value).context("Failed to stringify enablement JSON")?;
    let parent_dir = paths
        .enablement
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| paths.root.clone());
    let mut temp = tempfile::NamedTempFile::new_in(parent_dir)
        .context("Failed to create temporary file for enablement write")?;
    temp.write_all(json.as_bytes())
        .context("Failed to write enablement JSON")?;
    temp.write_all(b"\n").ok();
    temp.persist(&paths.enablement)
        .context("Failed to persist enablement file")?;
    Ok(())
}

fn schema() -> &'static JSONSchema {
    static SCHEMA: OnceCell<&'static JSONSchema> = OnceCell::new();
    SCHEMA.get_or_init(|| {
        let schema_value: Value =
            serde_json::from_str(SCHEMA_JSON).expect("embedded enablement schema is valid JSON");
        let leaked_value: &'static Value = Box::leak(Box::new(schema_value));
        let compiled = JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .compile(leaked_value)
            .expect("embedded enablement schema compiles");
        Box::leak(Box::new(compiled))
    })
}

fn format_pointer(pointer: &JSONPointer) -> String {
    let rendered = pointer.to_string();
    if rendered.is_empty() {
        "<root>".to_string()
    } else {
        rendered
    }
}

fn format_validation_errors<'a>(errors: impl IntoIterator<Item = ValidationError<'a>>) -> String {
    let mut parts = Vec::new();
    for error in errors {
        let path = format_pointer(&error.instance_path);
        parts.push(format!("{}: {}", path, error));
    }
    parts.join("; ")
}
