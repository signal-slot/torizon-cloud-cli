//! Human-readable and JSON output helpers.

use serde_json::Value;

/// Global output mode, selected by the top-level `--json` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
}

/// Print a value as pretty JSON to stdout.
pub fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{value}"),
    }
}

/// Render `rows` (each a JSON object) as a fixed-width text table, pulling the
/// given `columns` (key, header) in order. Missing/null values are shown
/// compactly.
pub fn print_table(rows: &[Value], columns: &[(&str, &str)]) {
    if rows.is_empty() {
        println!("(no results)");
        return;
    }

    let headers: Vec<&str> = columns.iter().map(|(_, h)| *h).collect();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();

    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(i, (key, _))| {
                    let s = cell_to_string(row.get(*key));
                    widths[i] = widths[i].max(s.chars().count());
                    s
                })
                .collect()
        })
        .collect();

    print_row(
        &headers.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
        &widths,
    );
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    print_row(&rule, &widths);
    for row in &cells {
        print_row(row, &widths);
    }
}

fn print_row(cells: &[String], widths: &[usize]) {
    let line: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let pad = widths[i].saturating_sub(c.chars().count());
            format!("{}{}", c, " ".repeat(pad))
        })
        .collect();
    println!("{}", line.join("  ").trim_end());
}

/// Convert a JSON cell into a compact display string.
fn cell_to_string(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "-".to_string(),
        Some(Value::String(s)) if s.is_empty() => "-".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|e| cell_to_string(Some(e)))
            .collect::<Vec<_>>()
            .join(","),
        Some(other) => other.to_string(),
    }
}

/// Report an operation that returns data (create/upload/launch). In JSON mode
/// only the raw response is printed so output stays machine-parseable.
pub fn report_data(format: Format, human_msg: &str, data: &Value) {
    match format {
        Format::Json => print_json(data),
        Format::Human => {
            println!("{human_msg}");
            print_json(data);
        }
    }
}

/// Report an operation with no meaningful response body (delete/cancel/membership).
pub fn report_status(format: Format, human_msg: &str, json_status: &Value) {
    match format {
        Format::Json => print_json(json_status),
        Format::Human => println!("{human_msg}"),
    }
}

/// Print a single JSON object as aligned `key: value` lines (human mode). Falls
/// back to pretty JSON for non-objects.
pub fn print_object(format: Format, value: &Value) {
    if format == Format::Json {
        print_json(value);
        return;
    }
    match value.as_object() {
        Some(map) => {
            let width = map.keys().map(|k| k.len()).max().unwrap_or(0);
            for (k, v) in map {
                let rendered = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => "-".to_string(),
                    other => other.to_string(),
                };
                println!("{k:<width$}  {rendered}");
            }
        }
        None => print_json(value),
    }
}

/// Extract the `values` array from a pagination wrapper, or treat the value
/// itself as the array.
pub fn paginated_values(v: &Value) -> Vec<Value> {
    match v.get("values").and_then(Value::as_array) {
        Some(arr) => arr.clone(),
        None => v.as_array().cloned().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cell_renders_scalars_and_arrays() {
        assert_eq!(cell_to_string(None), "-");
        assert_eq!(cell_to_string(Some(&Value::Null)), "-");
        assert_eq!(cell_to_string(Some(&json!(""))), "-");
        assert_eq!(cell_to_string(Some(&json!("hi"))), "hi");
        assert_eq!(cell_to_string(Some(&json!(true))), "true");
        assert_eq!(cell_to_string(Some(&json!(42))), "42");
        assert_eq!(cell_to_string(Some(&json!(["a", "b"]))), "a,b");
    }

    #[test]
    fn paginated_handles_wrapper_and_bare_array() {
        let wrapped = json!({ "values": [1, 2], "total": 2 });
        assert_eq!(paginated_values(&wrapped).len(), 2);
        let bare = json!([1, 2, 3]);
        assert_eq!(paginated_values(&bare).len(), 3);
        let neither = json!({ "foo": "bar" });
        assert!(paginated_values(&neither).is_empty());
    }
}
