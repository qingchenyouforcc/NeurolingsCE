//! JSON 写出工具：与 Qt `QJsonDocument::Compact` 字节级对齐的序列化。
//!
//! 规则：对象键按字典序排列（serde_json 默认即 BTreeMap）；浮点数以最短
//! 往返精度输出，整数值不带小数点（Rust Display 与 Qt 行为一致）；
//! 非有限浮点数输出 null；字符串按 JSON 标准转义。

use std::fmt::Write as _;

use serde_json::Value;

/// 把 JSON 值写成紧凑文本（无空白）。
pub fn write_compact(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() {
                    // Display 输出最短往返精度，整数不带小数点。
                    let _ = write!(out, "{f}");
                } else {
                    out.push_str("null");
                }
            } else {
                out.push_str("null");
            }
        }
        Value::String(s) => write_escaped(s, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_compact(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_escaped(key, out);
                out.push(':');
                write_compact(item, out);
            }
            out.push('}');
        }
    }
}

/// 紧凑序列化并返回字符串。
pub fn to_compact_string(value: &Value) -> String {
    let mut out = String::new();
    write_compact(value, &mut out);
    out
}

fn write_escaped(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Qt 文本模式的浮点格式：默认 6 位有效数字（%g 风格）。
/// 用于 CLI 文本输出的 Anchor 等字段。
pub fn format_g6(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".to_string()
        } else if value > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let abs = value.abs();
    let exponent = abs.log10().floor() as i32;
    // %g：指数 < -4 或 >= 精度(6) 时用科学计数法。
    if !(-4..6).contains(&exponent) {
        let s = format!("{value:.5e}");
        if let Some(e_pos) = s.find('e') {
            let (mantissa, exp) = s.split_at(e_pos);
            let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
            let exp_value: i32 = exp[1..].parse().unwrap_or(0);
            return format!("{mantissa}e{exp_value:+03}");
        }
        return s;
    }
    let decimals = (5 - exponent).max(0) as usize;
    let mut s = format!("{value:.decimals$}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integral_doubles_print_without_decimal_point() {
        assert_eq!(to_compact_string(&json!({"x": 368.0})), "{\"x\":368}");
        assert_eq!(to_compact_string(&json!({"x": 0.0})), "{\"x\":0}");
        assert_eq!(to_compact_string(&json!({"x": 67.2})), "{\"x\":67.2}");
    }

    #[test]
    fn keys_are_sorted_and_strings_escaped() {
        let value = json!({"b": 1, "a": "x\"y\n"});
        assert_eq!(to_compact_string(&value), "{\"a\":\"x\\\"y\\n\",\"b\":1}");
    }

    #[test]
    fn non_finite_becomes_null() {
        assert_eq!(to_compact_string(&json!(f64::NAN)), "null");
        assert_eq!(to_compact_string(&json!(f64::INFINITY)), "null");
    }

    #[test]
    fn g6_matches_qt_text_format() {
        assert_eq!(format_g6(0.0), "0");
        assert_eq!(format_g6(368.0), "368");
        assert_eq!(format_g6(225.63864462595598), "225.639");
        assert_eq!(format_g6(-12.3456789), "-12.3457");
        assert_eq!(format_g6(1234567.0), "1.23457e+06");
        assert_eq!(format_g6(0.00012), "0.00012");
    }
}
