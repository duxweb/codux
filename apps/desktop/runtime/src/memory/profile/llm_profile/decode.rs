use super::json::{json_object_candidates, llm_json_values};
use crate::ai_runtime::state::normalized_string;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::memory::profile) enum ProjectProfileDecodeError {
    EmptyResponse,
    NoJsonCandidate,
    MalformedJson,
    MissingProfileContent,
    InvalidProfileContent,
    ProfileTooLong,
}

impl std::fmt::Display for ProjectProfileDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ProjectProfileDecodeError::EmptyResponse => "empty LLM response",
            ProjectProfileDecodeError::NoJsonCandidate => "response did not contain a JSON object",
            ProjectProfileDecodeError::MalformedJson => "response contained malformed JSON",
            ProjectProfileDecodeError::MissingProfileContent => {
                "JSON did not contain project profile content"
            }
            ProjectProfileDecodeError::InvalidProfileContent => {
                "project profile content was missing required sections"
            }
            ProjectProfileDecodeError::ProfileTooLong => "project profile content exceeded limit",
        };
        formatter.write_str(message)
    }
}

pub(in crate::memory::profile) fn decode_project_profile_llm_response_detailed(
    raw: &str,
) -> std::result::Result<String, ProjectProfileDecodeError> {
    let stripped = strip_markdown_code_fences(raw);
    if stripped.trim().is_empty() {
        return Err(ProjectProfileDecodeError::EmptyResponse);
    }
    let (values, mut last_error) = project_profile_json_values(&stripped);
    if values.is_empty() {
        return Err(last_error);
    }
    for value in values {
        let Some(content) = project_profile_content_from_value(&value) else {
            last_error = ProjectProfileDecodeError::MissingProfileContent;
            continue;
        };
        match validate_project_profile_content(&content) {
            Ok(()) => return Ok(content),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn project_profile_json_values(raw: &str) -> (Vec<Value>, ProjectProfileDecodeError) {
    let values = llm_json_values(raw);
    let candidates = json_object_candidates(raw);
    if candidates.is_empty() {
        return (values, ProjectProfileDecodeError::NoJsonCandidate);
    }
    (values, ProjectProfileDecodeError::MalformedJson)
}

fn project_profile_content_from_value(value: &Value) -> Option<String> {
    if let Some(content) = string_from_keys(
        value,
        &[
            "content",
            "profile",
            "project_profile",
            "projectProfile",
            "projectProfileContent",
        ],
    ) {
        return Some(content);
    }
    for key in [
        "profile",
        "project_profile",
        "projectProfile",
        "result",
        "response",
        "data",
    ] {
        if let Some(content) = value.get(key).and_then(project_profile_content_from_value) {
            return Some(content);
        }
    }
    structured_project_profile_content(value)
}

fn structured_project_profile_content(value: &Value) -> Option<String> {
    let project = string_from_keys(
        value,
        &["project", "project_name", "projectName", "name", "title"],
    )?;
    let overview = string_from_keys(
        value,
        &[
            "overview",
            "summary",
            "description",
            "purpose",
            "project_overview",
            "projectOverview",
        ],
    )?;
    let mut sections = vec![
        format!("Project: {project}"),
        format!("Overview: {overview}"),
    ];
    push_project_profile_list_section(
        &mut sections,
        "Tech stack",
        list_from_keys(
            value,
            &[
                "tech_stack",
                "techStack",
                "stack",
                "technologies",
                "dependencies",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Common commands",
        list_from_keys(
            value,
            &[
                "common_commands",
                "commonCommands",
                "commands",
                "scripts",
                "dev_commands",
                "devCommands",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Top-level directories",
        list_from_keys(
            value,
            &[
                "top_level_directories",
                "topLevelDirectories",
                "directories",
                "folders",
            ],
        ),
    );
    push_project_profile_list_section(
        &mut sections,
        "Detected modules",
        list_from_keys(
            value,
            &[
                "detected_modules",
                "detectedModules",
                "modules",
                "areas",
                "components",
            ],
        ),
    );
    Some(sections.join("\n\n"))
}

fn push_project_profile_list_section(sections: &mut Vec<String>, title: &str, items: Vec<String>) {
    if items.is_empty() {
        return;
    }
    let lines = items
        .into_iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    sections.push(format!("{title}:\n{lines}"));
}

fn list_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let values = list_from_value(value);
        if !values.is_empty() {
            return values;
        }
    }
    Vec::new()
}

fn list_from_value(value: &Value) -> Vec<String> {
    if let Some(text) = value
        .as_str()
        .and_then(|value| normalized_string(Some(value)))
    {
        return text
            .lines()
            .filter_map(|line| {
                normalized_string(Some(line.trim_start_matches(['-', '*', ' '].as_slice())))
            })
            .collect();
    }
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .and_then(|value| normalized_string(Some(value)))
                        .or_else(|| string_from_keys(item, &["name", "label", "value", "command"]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn validate_project_profile_content(
    content: &str,
) -> std::result::Result<(), ProjectProfileDecodeError> {
    if content.len() > 16_000 {
        return Err(ProjectProfileDecodeError::ProfileTooLong);
    }
    if content.contains("Project:") && content.contains("Overview:") {
        return Ok(());
    }
    Err(ProjectProfileDecodeError::InvalidProfileContent)
}

fn strip_markdown_code_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    trimmed
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn string_from_keys(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|value| value.to_string()))
        })
        .and_then(|value| normalized_string(Some(&value)))
}
