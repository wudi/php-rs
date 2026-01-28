use regex::Regex;

#[derive(Debug, Clone)]
pub enum ExpectationType {
    Exact(String),
    Format(String),
    Regex(String),
}

/// Match actual output against expected output
pub fn match_output(actual: &str, expected: ExpectationType) -> bool {
    match expected {
        ExpectationType::Exact(expected_str) => {
            normalize_output(actual) == normalize_output(&expected_str)
        }
        ExpectationType::Format(format_str) => {
            match_expectf(actual, &format_str)
        }
        ExpectationType::Regex(regex_str) => {
            match_regex(actual, &regex_str)
        }
    }
}

/// Normalize output by trimming trailing whitespace and normalizing line endings
fn normalize_output(s: &str) -> String {
    s.replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

/// Match output against EXPECTF format (with %s, %d, %i, %f, etc.)
fn match_expectf(actual: &str, format_str: &str) -> bool {
    let regex_pattern = expectf_to_regex(format_str);
    match Regex::new(&regex_pattern) {
        Ok(re) => re.is_match(&normalize_output(actual)),
        Err(_) => false,
    }
}

/// Convert EXPECTF format to regex pattern
fn expectf_to_regex(format_str: &str) -> String {
    let mut pattern = String::new();
    let mut chars = format_str.chars().peekable();
    let mut in_regex = false;

    while let Some(ch) = chars.next() {
        if ch == '%' {
            if let Some(&next) = chars.peek() {
                match next {
                    's' => {
                        // %s - one or more non-newline characters
                        pattern.push_str(r"[^\r\n]+");
                        chars.next();
                    }
                    'S' => {
                        // %S - zero or more non-newline characters
                        pattern.push_str(r"[^\r\n]*");
                        chars.next();
                    }
                    'd' => {
                        // %d - unsigned integer
                        pattern.push_str(r"\d+");
                        chars.next();
                    }
                    'i' => {
                        // %i - signed integer
                        pattern.push_str(r"[+-]?\d+");
                        chars.next();
                    }
                    'x' => {
                        // %x - hexadecimal digits
                        pattern.push_str(r"[0-9a-fA-F]+");
                        chars.next();
                    }
                    'f' => {
                        // %f - float
                        // Simplified version without lookahead (which Rust regex doesn't support)
                        // Matches: 123, 123.456, .456, 123e10, 123.456e10, .456e10, etc.
                        pattern.push_str(r"[+-]?(?:\d+\.?\d*|\.\d+)(?:[Ee][+-]?\d+)?");
                        chars.next();
                    }
                    'c' => {
                        // %c - single character
                        pattern.push_str(".");
                        chars.next();
                    }
                    'e' => {
                        // %e - directory separator
                        pattern.push_str(r"[/\\]");
                        chars.next();
                    }
                    'a' => {
                        // %a - one or more characters (non-greedy)
                        pattern.push_str(".+?");
                        chars.next();
                    }
                    'A' => {
                        // %A - zero or more characters (non-greedy, including newlines)
                        pattern.push_str(".*?");
                        chars.next();
                    }
                    'w' => {
                        // %w - optional whitespace
                        pattern.push_str(r"\s*");
                        chars.next();
                    }
                    '0' => {
                        // %0 - null byte
                        pattern.push_str(r"\x00");
                        chars.next();
                    }
                    'r' => {
                        // %r...%r - regex delimiter
                        if in_regex {
                            // End of regex section
                            in_regex = false;
                        } else {
                            // Start of regex section
                            in_regex = true;
                        }
                        chars.next();
                    }
                    _ => {
                        pattern.push(ch);
                    }
                }
            } else {
                pattern.push(ch);
            }
        } else if in_regex {
            // Inside %r...%r, use content as-is (it's already regex)
            pattern.push(ch);
        } else {
            // Escape regex special characters
            // Same as PHP's preg_quote with '/' delimiter
            if "\\^$.|?*+()[{:/".contains(ch) {
                pattern.push('\\');
            }
            pattern.push(ch);
        }
    }

    // Anchor the pattern (no normalization needed for the pattern itself)
    format!("^{}$", pattern)
}

/// Match output against regex pattern
fn match_regex(actual: &str, regex_str: &str) -> bool {
    match Regex::new(regex_str) {
        Ok(re) => re.is_match(&normalize_output(actual)),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(match_output("Hello World", ExpectationType::Exact("Hello World".to_string())));
        assert!(match_output("Hello World\n", ExpectationType::Exact("Hello World".to_string())));
        assert!(!match_output("Hello", ExpectationType::Exact("Hello World".to_string())));
    }

    #[test]
    fn test_expectf_string() {
        let format = "Error: %s at line %d".to_string();
        assert!(match_output(
            "Error: file not found at line 42",
            ExpectationType::Format(format.clone())
        ));
        assert!(!match_output(
            "Error: 123 at line abc",
            ExpectationType::Format(format)
        ));
    }

    #[test]
    fn test_expectf_integer() {
        // %d is unsigned integer
        let format_unsigned = "Value: %d".to_string();
        assert!(match_output("Value: 42", ExpectationType::Format(format_unsigned.clone())));
        assert!(!match_output("Value: -42", ExpectationType::Format(format_unsigned.clone())));
        assert!(!match_output("Value: abc", ExpectationType::Format(format_unsigned)));
        
        // %i is signed integer
        let format_signed = "Value: %i".to_string();
        assert!(match_output("Value: 42", ExpectationType::Format(format_signed.clone())));
        assert!(match_output("Value: -42", ExpectationType::Format(format_signed.clone())));
        assert!(!match_output("Value: abc", ExpectationType::Format(format_signed)));
    }

    #[test]
    fn test_expectf_float() {
        let format = "Pi: %f".to_string();
        assert!(match_output("Pi: 3.14159", ExpectationType::Format(format.clone())));
        assert!(match_output("Pi: 3", ExpectationType::Format(format.clone())));
        assert!(match_output("Pi: -3.14", ExpectationType::Format(format.clone())));
        assert!(match_output("Pi: 1.5e10", ExpectationType::Format(format.clone())));
    }

    #[test]
    fn test_regex_match() {
        let regex = r"^\d{4}-\d{2}-\d{2}$".to_string();
        assert!(match_output("2024-01-15", ExpectationType::Regex(regex.clone())));
        assert!(!match_output("2024-1-15", ExpectationType::Regex(regex)));
    }
}
