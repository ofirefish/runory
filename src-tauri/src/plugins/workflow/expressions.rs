use std::collections::HashMap;

use crate::plugins::error::{PluginError, PluginResult};

/// Resolve `{{ inputs.x }}`, `{{ host.id }}`, `{{ host.os }}`, `{{ steps.ID.outputs.KEY }}`.
/// Safe subset only: no JS, no function calls, no shell substitution.
pub fn resolve_expression(
    template: &str,
    inputs: &serde_json::Value,
    host_id: Option<&str>,
    host_os: Option<&str>,
    step_outputs: &HashMap<String, serde_json::Value>,
) -> PluginResult<String> {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (before, after_open) = rest.split_at(start);
        result.push_str(before);
        let Some(end) = after_open.find("}}") else {
            return Err(PluginError::WorkflowValidation(
                "unclosed expression {{".into(),
            ));
        };
        let expr = after_open[2..end].trim();
        let value = resolve_path(expr, inputs, host_id, host_os, step_outputs)?;
        result.push_str(&value);
        rest = &after_open[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

fn resolve_path(
    path: &str,
    inputs: &serde_json::Value,
    host_id: Option<&str>,
    host_os: Option<&str>,
    step_outputs: &HashMap<String, serde_json::Value>,
) -> PluginResult<String> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["inputs", key] => json_to_string(inputs.get(*key).unwrap_or(&serde_json::Value::Null)),
        ["host", "id"] => Ok(host_id.unwrap_or("").to_owned()),
        ["host", "os"] => Ok(host_os.unwrap_or("").to_owned()),
        ["steps", step_id, "outputs", key] => {
            let outputs = step_outputs.get(*step_id).ok_or_else(|| {
                PluginError::WorkflowValidation(format!("unknown step outputs: {step_id}"))
            })?;
            json_to_string(outputs.get(*key).unwrap_or(&serde_json::Value::Null))
        }
        _ => Err(PluginError::WorkflowValidation(format!(
            "unsupported expression path: {path}"
        ))),
    }
}

fn json_to_string(value: &serde_json::Value) -> PluginResult<String> {
    match value {
        serde_json::Value::Null => Ok(String::new()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => Ok(s.clone()),
        other => Err(PluginError::WorkflowValidation(format!(
            "expression resolved to non-scalar value: {other}"
        ))),
    }
}

/// Evaluate a simple condition: equality, contains, or boolean literal.
pub fn evaluate_condition(
    condition: &str,
    inputs: &serde_json::Value,
    host_id: Option<&str>,
    host_os: Option<&str>,
    step_outputs: &HashMap<String, serde_json::Value>,
) -> PluginResult<bool> {
    let trimmed = condition.trim();
    if trimmed.is_empty() {
        return Ok(true);
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Ok(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Ok(false);
    }

    if let Some((left, right)) = split_op(trimmed, " contains ") {
        let left_val = resolve_path(left.trim(), inputs, host_id, host_os, step_outputs)?;
        let right_val =
            parse_literal_or_path(right.trim(), inputs, host_id, host_os, step_outputs)?;
        return Ok(left_val.contains(&right_val));
    }
    if let Some((left, right)) = split_op(trimmed, "==") {
        let left_val = resolve_path(left.trim(), inputs, host_id, host_os, step_outputs)?;
        let right_val =
            parse_literal_or_path(right.trim(), inputs, host_id, host_os, step_outputs)?;
        return Ok(left_val == right_val);
    }
    if let Some((left, right)) = split_op(trimmed, "!=") {
        let left_val = resolve_path(left.trim(), inputs, host_id, host_os, step_outputs)?;
        let right_val =
            parse_literal_or_path(right.trim(), inputs, host_id, host_os, step_outputs)?;
        return Ok(left_val != right_val);
    }

    Err(PluginError::WorkflowValidation(format!(
        "unsupported condition: {trimmed}"
    )))
}

fn parse_literal_or_path(
    token: &str,
    inputs: &serde_json::Value,
    host_id: Option<&str>,
    host_os: Option<&str>,
    step_outputs: &HashMap<String, serde_json::Value>,
) -> PluginResult<String> {
    if (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with('\'') && token.ends_with('\''))
    {
        return Ok(token[1..token.len() - 1].to_owned());
    }
    if token.eq_ignore_ascii_case("true") || token.eq_ignore_ascii_case("false") {
        return Ok(token.to_ascii_lowercase());
    }
    if token
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
    {
        return Ok(token.to_owned());
    }
    resolve_path(token, inputs, host_id, host_os, step_outputs)
}

fn split_op<'a>(input: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let idx = input.find(op)?;
    let (left, right) = input.split_at(idx);
    Some((left, &right[op.len()..]))
}

/// Evaluate host-menu `when` expressions matching the allowlist in manifest validator.
pub fn evaluate_when(
    when: &str,
    host_os: Option<&str>,
    host_arch: Option<&str>,
    tags: &[String],
) -> PluginResult<bool> {
    let trimmed = when.trim();
    let os = host_os.unwrap_or("");
    let arch = host_arch.unwrap_or("");
    match trimmed {
        "host.os == \"linux\"" | "host.os == 'linux'" => Ok(os.eq_ignore_ascii_case("linux")),
        "host.os == \"windows\"" | "host.os == 'windows'" => Ok(os.eq_ignore_ascii_case("windows")),
        "host.arch == \"x86_64\"" | "host.arch == 'x86_64'" => {
            Ok(arch == "x86_64" || arch == "amd64")
        }
        "host.tags contains \"production\"" | "host.tags contains 'production'" => {
            Ok(tags.iter().any(|t| t == "production"))
        }
        _ => Err(PluginError::WorkflowValidation(format!(
            "unsupported when expression: {trimmed}"
        ))),
    }
}

/// Deep-resolve string leaves in a JSON value that contain `{{ ... }}` expressions.
pub fn resolve_json_value(
    value: &serde_json::Value,
    inputs: &serde_json::Value,
    host_id: Option<&str>,
    host_os: Option<&str>,
    step_outputs: &HashMap<String, serde_json::Value>,
) -> PluginResult<serde_json::Value> {
    match value {
        serde_json::Value::String(s) if s.contains("{{") => Ok(serde_json::Value::String(
            resolve_expression(s, inputs, host_id, host_os, step_outputs)?,
        )),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_json_value(
                    item,
                    inputs,
                    host_id,
                    host_os,
                    step_outputs,
                )?);
            }
            Ok(serde_json::Value::Array(out))
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(
                    k.clone(),
                    resolve_json_value(v, inputs, host_id, host_os, step_outputs)?,
                );
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_inputs_and_host() {
        let inputs = serde_json::json!({ "name": "alice" });
        let steps = HashMap::new();
        let out = resolve_expression(
            "hi {{ inputs.name }} on {{ host.os }}",
            &inputs,
            Some("h1"),
            Some("linux"),
            &steps,
        )
        .unwrap();
        assert_eq!(out, "hi alice on linux");
    }

    #[test]
    fn evaluates_equality_condition() {
        let inputs = serde_json::json!({});
        let steps = HashMap::new();
        assert!(
            evaluate_condition("host.os == \"linux\"", &inputs, None, Some("linux"), &steps)
                .unwrap()
        );
    }

    #[test]
    fn evaluate_when_tags() {
        assert!(evaluate_when(
            "host.tags contains 'production'",
            Some("linux"),
            Some("x86_64"),
            &["production".into()]
        )
        .unwrap());
    }
}
