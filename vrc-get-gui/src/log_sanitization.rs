use url::Url;

pub fn sanitize_url(value: &str) -> String {
    Url::parse(value)
        .map(|mut url| {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        })
        .unwrap_or_else(|_| "<invalid url>".to_string())
}

pub fn sanitize_log_text(message: &str) -> String {
    let mut result = String::with_capacity(message.len());
    let mut token = String::new();
    let mut state = SanitizationState::default();

    for ch in message.chars() {
        if ch.is_whitespace() {
            if !token.is_empty() {
                result.push_str(&sanitize_log_token(&token, &mut state));
                token.clear();
            }
            result.push(ch);
        } else {
            token.push(ch);
        }
    }

    if !token.is_empty() {
        result.push_str(&sanitize_log_token(&token, &mut state));
    }

    result
}

#[derive(Default)]
struct SanitizationState {
    redact_next_count: usize,
    pending_sensitive_assignment_next_tokens: Option<usize>,
}

fn sanitize_log_token(token: &str, state: &mut SanitizationState) -> String {
    if state.redact_next_count > 0 {
        state.redact_next_count -= 1;
        state.pending_sensitive_assignment_next_tokens = None;
        return redacted_token_preserving_trailing_delimiters(token);
    }

    let token = sanitize_urls_in_token(token);
    let lower = token.to_lowercase();
    if let Some(next_tokens) = state.pending_sensitive_assignment_next_tokens.take() {
        if is_assignment_separator_token(&token) {
            state.redact_next_count = next_tokens;
            return token;
        }
    }

    if token_starts_with_secret_prefix(&lower) {
        "<redacted>".to_string()
    } else if lower == "bearer" {
        state.redact_next_count = 1;
        token
    } else if let Some((redacted, redact_next)) = redact_sensitive_assignment(&token) {
        state.redact_next_count = redact_next;
        redacted
    } else if let Some(next_tokens) = sensitive_key_token_next_tokens(&token) {
        state.pending_sensitive_assignment_next_tokens = Some(next_tokens);
        token
    } else {
        token
    }
}

fn sanitize_urls_in_token(mut token: &str) -> String {
    let mut result = String::new();
    while let Some((prefix, url, suffix)) = split_url_token(token) {
        result.push_str(prefix);
        result.push_str(&sanitize_url(url));
        token = suffix;
    }
    result.push_str(token);
    result
}

fn split_url_token(token: &str) -> Option<(&str, &str, &str)> {
    let scheme_separator = token.find("://")?;
    let scheme_start = token[..scheme_separator]
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!is_url_scheme_char(ch)).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let candidate_with_suffix = &token[scheme_start..];
    let url_end = candidate_with_suffix
        .char_indices()
        .skip(1)
        .find_map(|(index, ch)| is_url_trailing_delimiter(ch).then_some(index))
        .unwrap_or(candidate_with_suffix.len());
    let candidate = &candidate_with_suffix[..url_end];
    Url::parse(candidate).ok()?;

    Some((
        &token[..scheme_start],
        candidate,
        &candidate_with_suffix[url_end..],
    ))
}

fn is_url_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')
}

fn is_url_trailing_delimiter(ch: char) -> bool {
    matches!(ch, '"' | '\'' | ',' | ';' | '}' | ')' | '>')
}

fn token_starts_with_secret_prefix(lower: &str) -> bool {
    lower
        .trim_start_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .starts_with("sk-")
}

const SENSITIVE_ASSIGNMENT_KEYS: &[(&str, usize)] = &[
    ("authorization", 2),
    ("token", 1),
    ("secret", 1),
    ("api_key", 1),
    ("api-key", 1),
    ("x-api-key", 1),
    ("apikey", 1),
    ("password", 1),
    ("passwd", 1),
    ("pwd", 1),
];

fn redact_sensitive_assignment(token: &str) -> Option<(String, usize)> {
    for (key, next_tokens) in SENSITIVE_ASSIGNMENT_KEYS {
        if let Some((separator_end, value)) = sensitive_assignment_parts(token, key) {
            let prefix = &token[..separator_end];
            let trailing = sensitive_value_trailing_delimiters(value);
            if value.len() == trailing.len() {
                return Some((format!("{prefix}<redacted>"), *next_tokens));
            }
            let redact_next = if *key == "authorization"
                && authorization_bearer_value_needs_next_token_redaction(value, trailing)
            {
                1
            } else {
                0
            };
            return Some((format!("{prefix}<redacted>{trailing}"), redact_next));
        }
    }
    None
}

fn sensitive_key_token_next_tokens(token: &str) -> Option<usize> {
    let candidate = token.trim_matches(|ch| {
        matches!(
            ch,
            '"' | '\'' | '{' | '[' | '(' | '}' | ']' | ')' | ',' | ';'
        )
    });
    SENSITIVE_ASSIGNMENT_KEYS
        .iter()
        .find_map(|(key, next_tokens)| candidate.eq_ignore_ascii_case(key).then_some(*next_tokens))
}

fn is_assignment_separator_token(token: &str) -> bool {
    token
        .trim_matches(|ch| matches!(ch, '"' | '\'' | ',' | ';'))
        .chars()
        .all(|ch| matches!(ch, ':' | '='))
}

fn redacted_token_preserving_trailing_delimiters(token: &str) -> String {
    let trailing = sensitive_value_trailing_delimiters(token);
    if token.len() == trailing.len() {
        "<redacted>".to_string()
    } else {
        format!("<redacted>{trailing}")
    }
}

fn authorization_bearer_value_needs_next_token_redaction(value: &str, trailing: &str) -> bool {
    if !trailing.is_empty() {
        return false;
    }

    value
        .trim_start_matches(|ch: char| matches!(ch, '"' | '\'' | '{' | '[' | '(' | ' ' | '\t'))
        .eq_ignore_ascii_case("bearer")
}

fn sensitive_assignment_parts<'a>(token: &'a str, key: &str) -> Option<(usize, &'a str)> {
    for (key_start, _) in token.char_indices() {
        let after_key = key_start + key.len();
        let Some(candidate) = token.get(key_start..after_key) else {
            continue;
        };
        if !candidate.eq_ignore_ascii_case(key) {
            continue;
        }

        let Some((relative_separator, separator_ch)) = token[after_key..]
            .char_indices()
            .find(|(_, ch)| matches!(ch, ':' | '='))
        else {
            continue;
        };
        let separator = after_key + relative_separator;
        if !token[after_key..separator]
            .chars()
            .all(|ch| matches!(ch, '"' | '\'' | ' ' | '\t'))
        {
            continue;
        }

        let separator_end = separator + separator_ch.len_utf8();
        return Some((separator_end, &token[separator_end..]));
    }

    None
}

fn sensitive_value_trailing_delimiters(value: &str) -> &str {
    let start = value
        .char_indices()
        .rev()
        .find_map(|(index, ch)| {
            (!matches!(ch, '"' | '\'' | ',' | ';' | '}' | ']' | ')' | ' ' | '\t'))
                .then_some(index + ch.len_utf8())
        })
        .unwrap_or(0);
    &value[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_url_removes_query_fragment_and_userinfo() {
        assert_eq!(
            sanitize_url("https://user:pass@example.com/path?token=secret#frag"),
            "https://example.com/path"
        );
    }

    #[test]
    fn sanitize_log_text_removes_embedded_url_credentials_and_query() {
        let redacted = sanitize_log_text(
            r#"failed url="https://user:pass@example.com/path?token=secret&x=1") next"#,
        );

        assert_eq!(redacted, r#"failed url="https://example.com/path") next"#);
    }

    #[test]
    fn sanitize_log_text_removes_deep_link_query() {
        let redacted = sanitize_log_text(
            "Invalid deep link: vcc://vpm/addRepo?url=https%3A%2F%2Fexample.com%2Fvpm.json&headers[]=Authorization:secret",
        );

        assert_eq!(redacted, "Invalid deep link: vcc://vpm/addRepo");
    }

    #[test]
    fn sanitize_log_text_removes_multiple_urls_in_one_token() {
        let redacted = sanitize_log_text(
            r#"{"one":"https://user:pass@example.com/a?token=secret","two":"https://user:pass@example.org/b?key=secret"}"#,
        );

        assert_eq!(
            redacted,
            r#"{"one":"https://example.com/a","two":"https://example.org/b"}"#
        );
    }

    #[test]
    fn sanitize_log_text_redacts_sensitive_values() {
        let redacted = sanitize_log_text(
            r#"{"token":"json-secret"} api_key: plain-secret "sk-project" Authorization: Bearer abcdefghijklmnopqrstuvwxyz"#,
        );

        assert!(!redacted.contains("json-secret"));
        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("sk-project"));
        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn sanitize_log_text_redacts_hyphenated_api_keys() {
        let redacted = sanitize_log_text(
            r#"api-key=plain-secret x-api-key: header-secret {"x-api-key":"json-secret"}"#,
        );

        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("header-secret"));
        assert!(!redacted.contains("json-secret"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn sanitize_log_text_redacts_spaced_sensitive_assignments() {
        let redacted = sanitize_log_text(
            "token = plain-secret api-key = api-secret authorization = Bearer abc",
        );

        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("api-secret"));
        assert!(!redacted.contains("Bearer"));
        assert!(!redacted.contains("abc"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn sanitize_log_text_redacts_password_keys() {
        let redacted =
            sanitize_log_text(r#"password=plain-secret passwd: other-secret pwd = third-secret"#);

        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("other-secret"));
        assert!(!redacted.contains("third-secret"));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn sanitize_log_text_handles_unicode_before_sensitive_key() {
        let redacted = sanitize_log_text("prefix İtoken=secret next");

        assert_eq!(redacted, "prefix İtoken=<redacted> next");
    }
}
