use std::collections::HashMap;

pub fn contains_verification_code_placeholder(s: &str) -> bool {
    s.contains("{{验证码}}") || s.contains("{{verification_code}}")
}

pub fn is_standalone_verification_code_line(line: &str) -> bool {
    let rem = line
        .replace("{{验证码}}", "")
        .replace("{{verification_code}}", "");
    let trimmed = rem.trim();

    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let keywords = [
        "验证码",
        "动态验证码",
        "verification code",
        "verification_code",
        "code",
        "otp",
        "captcha",
        "passcode",
    ];

    let mut stripped = lower;
    for kw in keywords {
        stripped = stripped.replace(kw, "");
    }

    stripped.chars().all(|c| {
        c.is_whitespace()
            || matches!(
                c,
                ':' | '：'
                    | '-'
                    | '—'
                    | '|'
                    | ','
                    | '，'
                    | ';'
                    | '；'
                    | '['
                    | ']'
                    | '【'
                    | '】'
                    | '('
                    | ')'
                    | '（'
                    | '）'
                    | '"'
                    | '\''
                    | '📱'
                    | '💬'
                    | '🔑'
                    | '🔒'
            )
    })
}

pub fn clean_inline_verification_code_placeholder(text: &str) -> String {
    let mut result = text.to_string();

    let bracketed_patterns = [
        "（验证码: {{验证码}}）",
        "（验证码：{{验证码}}）",
        "（验证码{{验证码}}）",
        "(验证码: {{验证码}})",
        "(验证码：{{验证码}})",
        "(验证码{{验证码}})",
        "【验证码: {{验证码}}】",
        "【验证码：{{验证码}}】",
        "[验证码: {{验证码}}]",
        "[验证码：{{验证码}}]",
        "(Code: {{verification_code}})",
        "(Verification Code: {{verification_code}})",
        "（verification_code: {{verification_code}}）",
    ];
    for p in bracketed_patterns {
        result = result.replace(p, "");
    }

    let prefix_patterns = [
        "：验证码: {{验证码}}",
        "：验证码：{{验证码}}",
        "：验证码{{验证码}}",
        ": 验证码: {{验证码}}",
        ": 验证码：{{验证码}}",
        ": 验证码{{验证码}}",
        " - 验证码: {{验证码}}",
        " - 验证码：{{验证码}}",
        " - 验证码{{验证码}}",
        " | 验证码: {{验证码}}",
        " | 验证码：{{验证码}}",
        " | 验证码{{验证码}}",
        "，验证码: {{验证码}}",
        "，验证码：{{验证码}}",
        ", 验证码: {{验证码}}",
        ", 验证码：{{验证码}}",
        "；验证码: {{验证码}}",
        "；验证码：{{验证码}}",
        "; 验证码: {{验证码}}",
        "; 验证码：{{验证码}}",
        "验证码: {{验证码}}",
        "验证码：{{验证码}}",
        "验证码{{验证码}}",
        "：Code: {{verification_code}}",
        ": Code: {{verification_code}}",
        " - Code: {{verification_code}}",
        " | Code: {{verification_code}}",
        ", Code: {{verification_code}}",
        "：{{verification_code}}",
        ": {{verification_code}}",
        " - {{verification_code}}",
        " | {{verification_code}}",
        "，{{verification_code}}",
        ", {{verification_code}}",
        "；{{verification_code}}",
        "; {{verification_code}}",
    ];
    for p in prefix_patterns {
        result = result.replace(p, "");
    }

    let english_patterns = [
        "(Verification Code: {{verification_code}})",
        "[Verification Code: {{verification_code}}]",
        "(Code: {{verification_code}})",
        "[Code: {{verification_code}}]",
        "Verification Code: {{verification_code}}, ",
        "Verification Code: {{verification_code}}",
        "Code: {{verification_code}}, ",
        "Code: {{verification_code}}",
        "Code {{verification_code}}",
        "code: {{verification_code}}",
    ];
    for p in english_patterns {
        result = result.replace(p, "");
    }

    result
        .replace("{{验证码}}", "")
        .replace("{{verification_code}}", "")
}

pub fn clean_empty_verification_code_template(template: &str) -> String {
    if !contains_verification_code_placeholder(template) {
        return template.to_string();
    }

    if template.contains('\n') {
        let mut lines = Vec::new();
        for line in template.lines() {
            let trimmed_line = line.trim();
            if contains_verification_code_placeholder(trimmed_line) {
                if is_standalone_verification_code_line(trimmed_line) {
                    continue;
                }
                lines.push(clean_inline_verification_code_placeholder(trimmed_line));
            } else {
                lines.push(line.to_string());
            }
        }
        return lines.join("\n");
    }

    let joined = template.to_string();
    clean_inline_verification_code_placeholder(&joined)
}

pub fn render_sms_template(template: &str, variables: &HashMap<&str, &str>) -> String {
    let mut rendered = template.to_string();
    for (k, v) in variables {
        rendered = rendered.replace(&format!("{{{{{k}}}}}"), v);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_template() {
        let tpl = "【短信通知】\n发件人：{{sender}}\n验证码：{{verification_code}}\n时间：{{time}}";
        let cleaned = clean_empty_verification_code_template(tpl);
        assert!(!cleaned.contains("验证码"));
        assert_eq!(cleaned, "【短信通知】\n发件人：{{sender}}\n时间：{{time}}");
    }
}
