use serde_json::{Map, Value};

pub fn parse_brew_packages(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect()
}

pub fn parse_npm_packages(output: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return Vec::new();
    };
    dependencies(&value).map(entries).unwrap_or_default()
}

pub fn parse_pnpm_packages(output: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return Vec::new();
    };
    let root = value
        .as_array()
        .and_then(|array| array.first())
        .unwrap_or(&value);
    let mut result = Vec::new();
    for key in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(object) = root.get(key).and_then(Value::as_object) {
            result.extend(entries(object));
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result.dedup_by(|left, right| left.0 == right.0);
    result
}

pub fn parse_uv_packages(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            if line.starts_with('-') || line.starts_with(' ') || line.trim().is_empty() {
                return None;
            }
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_string();
            let version = parts.next()?.trim_start_matches('v').to_string();
            Some((name, version))
        })
        .collect()
}

pub fn parse_pip_packages(output: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return Vec::new();
    };
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item.get("name")?.as_str()?.to_string(),
                item.get("version")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn dependencies(value: &Value) -> Option<&Map<String, Value>> {
    value.get("dependencies").and_then(Value::as_object)
}

fn entries(object: &Map<String, Value>) -> Vec<(String, String)> {
    object
        .iter()
        .filter_map(|(name, metadata)| {
            let version = metadata
                .get("version")
                .and_then(Value::as_str)
                .or_else(|| metadata.as_str())?;
            Some((name.clone(), version.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brew_output() {
        assert_eq!(
            parse_brew_packages("git 2.49.0\nripgrep 14.1.1\n"),
            vec![
                ("git".into(), "2.49.0".into()),
                ("ripgrep".into(), "14.1.1".into())
            ]
        );
    }

    #[test]
    fn parses_npm_json() {
        let values = parse_npm_packages(r#"{"dependencies":{"typescript":{"version":"5.9.2"}}}"#);
        assert_eq!(values, vec![("typescript".into(), "5.9.2".into())]);
    }

    #[test]
    fn parses_uv_tool_list() {
        assert_eq!(
            parse_uv_packages("black v25.1.0\n- black\nhttpx v0.28.1\n"),
            vec![
                ("black".into(), "25.1.0".into()),
                ("httpx".into(), "0.28.1".into())
            ]
        );
    }
}
