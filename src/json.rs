//! Gson's view of JSON, as the original reads its rule files: `JsonParser.parse` (lenient) and the
//! `JsonElement.getAs*` conversions.
use serde_json::Value;

/// A rule file's error, as the original's exception message reaches `ErrorHandler`.
pub type Error = String;

/// The text without the comments Gson's lenient reader skips: `//` and `#` to the line's end and
/// `/* */`, outside strings.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' => {
                quote = Some(c);
                out.push(c);
            }
            '#' => skip_line(&mut chars),
            '/' if chars.peek() == Some(&'/') => skip_line(&mut chars),
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

fn skip_line(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in chars.by_ref() {
        if c == '\n' {
            break;
        }
    }
}

/// `JsonParser.parse` of a whole rule file. Gson's other lenient forms (unquoted names, single
/// quotes, `;` and `=>`) are not read.
pub fn parse_file(text: &str) -> Result<Value, Error> {
    serde_json::from_str(&without_comments(text)).map_err(|e| e.to_string())
}

/// What a JSON-typed attribute's text parses to with `JsonParser.parse`: JSON, or one unquoted
/// word, which Gson's lenient reader takes as a string.
pub fn parse_value(text: &str) -> Result<Value, Error> {
    if let Ok(value) = serde_json::from_str(text) {
        return Ok(value);
    }
    let word = text.trim();
    let literal = !word.is_empty()
        && !word
            .chars()
            .any(|c| c.is_whitespace() || "/\\;#={}[]:,\"'".contains(c));
    if literal {
        Ok(Value::String(word.to_owned()))
    } else {
        Err(format!("Malformed JSON: {text}"))
    }
}

/// `JsonElement.getAsString`: a string's text, a number or boolean as text, a one-element array's
/// element.
pub fn as_string(value: &Value) -> Result<String, Error> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Array(a) if a.len() == 1 => as_string(&a[0]),
        _ => Err(format!("Not a string: {value}")),
    }
}

/// `JsonElement.getAsInt`: a number's `intValue`, or a string through `Integer.parseInt`.
pub fn as_int(value: &Value) -> Result<i32, Error> {
    match value {
        Value::Number(n) => Ok(match n.as_i64() {
            Some(i) => i as i32,
            None => n.as_f64().unwrap_or_default() as i32,
        }),
        Value::Array(a) if a.len() == 1 => as_int(&a[0]),
        _ => {
            let s = as_string(value)?;
            s.parse::<i32>()
                .map_err(|_| format!("For input string: \"{s}\""))
        }
    }
}

/// `JsonElement.getAsFloat`.
pub fn as_float(value: &Value) -> Result<f32, Error> {
    match value {
        Value::Number(n) => Ok(n.as_f64().unwrap_or_default() as f32),
        Value::Array(a) if a.len() == 1 => as_float(&a[0]),
        _ => {
            let s = as_string(value)?;
            s.trim()
                .parse::<f32>()
                .map_err(|_| format!("For input string: \"{s}\""))
        }
    }
}

/// `JsonElement.getAsBoolean`: a boolean, or `Boolean.parseBoolean` of the text.
pub fn as_bool(value: &Value) -> Result<bool, Error> {
    match value {
        Value::Bool(b) => Ok(*b),
        Value::Array(a) if a.len() == 1 => as_bool(&a[0]),
        _ => Ok(as_string(value)?.eq_ignore_ascii_case("true")),
    }
}

/// `JSonTools.asArrayOrSingle`.
pub fn array_or_single(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(a) => a.iter().collect(),
        v => vec![v],
    }
}

/// `ResourceLocation.parse(s).toString()`: the id with `minecraft` when it names no namespace.
pub fn id(s: &str) -> String {
    if s.contains(':') {
        s.to_owned()
    } else {
        format!("minecraft:{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_are_skipped_outside_strings() {
        let text = "[ // a rule\n {\"mob\": \"a#b\", /* x */ \"mincount\": 3} # end\n]";
        let value = parse_file(text).unwrap();
        assert_eq!(value[0]["mob"], "a#b");
        assert_eq!(value[0]["mincount"], 3);
    }

    #[test]
    fn a_json_attribute_reads_as_gson_does() {
        assert_eq!(parse_value("15").unwrap(), 15);
        assert_eq!(parse_value("abc").unwrap(), "abc");
        assert!(parse_value("15,minecraft:zombie").is_err());
        assert_eq!(as_int(&Value::String("7".into())).unwrap(), 7);
        assert!(as_bool(&Value::String("TRUE".into())).unwrap());
        assert!(!as_bool(&Value::String("yes".into())).unwrap());
    }
}
