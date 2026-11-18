use std::collections::HashMap;

pub fn contains_verification_code_placeholder(s: &str) -> bool {
    s.contains("{{验证码}}") || s.contains("{{verification_code}}")
}

pub fn is_standalone_verification_code_line(line: &str) -> bool {
    let rem = line
        .replace("{{验证码}}", "")
        .replace("{{verification_code}}", "");
    // 若同一行还有其他模板占位符，则不能整行删除
    if rem.contains("{{") && rem.contains("}}") {
        return false;
    }

    let trimmed = rem.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let keywords = [
        "动态验证码",
        "验证码",
        "校验码",
        "动态码",
        "确认码",
        "verification code",
        "verification_code",
        "passcode",
        "captcha",
        "code",
        "otp",
    ];

    let mut stripped = lower;
    let mut had_keyword = false;
    for kw in keywords {
        if stripped.contains(kw) {
            had_keyword = true;
            stripped = stripped.replace(kw, "");
        }
    }

    if had_keyword {
        for wrapper in ["您的", "你的", "本次", "是", "为"] {
            stripped = stripped.replace(wrapper, "");
        }
    }

    // 其余仅允许空白、标点、括号及 Emoji/装饰符号（非字母/数字/汉字）
    stripped.chars().all(|c| !c.is_alphanumeric())
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
        "，您的验证码是：【{{验证码}}】",
        ", 您的验证码是：【{{验证码}}】",
        "您的验证码是：【{{验证码}}】",
        "【{{验证码}}】",
        "（{{验证码}}）",
        "({{验证码}})",
        "[{{验证码}}]",
        "(Code: {{verification_code}})",
        "(Verification Code: {{verification_code}})",
        "（verification_code: {{verification_code}}）",
        "【{{verification_code}}】",
        "（{{verification_code}}）",
        "({{verification_code}})",
        "[{{verification_code}}]",
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
        "，验证码{{验证码}}",
        ", 验证码: {{验证码}}",
        ", 验证码：{{验证码}}",
        ", 验证码{{验证码}}",
        "；验证码: {{验证码}}",
        "；验证码：{{验证码}}",
        "；验证码{{验证码}}",
        "; 验证码: {{验证码}}",
        "; 验证码：{{验证码}}",
        "; 验证码{{验证码}}",
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
        "：{{验证码}}",
        ": {{验证码}}",
        " - {{验证码}}",
        " | {{验证码}}",
        "，{{验证码}}",
        ", {{验证码}}",
        "；{{验证码}}",
        "; {{验证码}}",
    ];
    for p in prefix_patterns {
        result = result.replace(p, "");
    }

    let suffix_patterns = [
        "您的验证码是：",
        "您的验证码是:",
        "您的验证码是",
        "验证码: {{验证码}}，",
        "验证码：{{验证码}}，",
        "验证码: {{验证码}}, ",
        "验证码：{{验证码}}, ",
        "验证码: {{验证码}}；",
        "验证码：{{验证码}}；",
        "验证码: {{验证码}}; ",
        "验证码：{{验证码}}; ",
        "验证码: {{验证码}} - ",
        "验证码：{{验证码}} - ",
        "验证码: {{验证码}} | ",
        "验证码：{{验证码}} | ",
        "验证码: {{验证码}}",
        "验证码：{{验证码}}",
        "验证码 {{验证码}}",
        "验证码为{{验证码}}",
        "验证码是{{验证码}}",
        "验证码{{验证码}}",
        "动态验证码: {{验证码}}",
        "动态验证码：{{验证码}}",
        "动态验证码{{验证码}}",
        "Verification Code: {{verification_code}}, ",
        "Verification Code: {{verification_code}}",
        "Code: {{verification_code}}, ",
        "Code: {{verification_code}}",
        "Code {{verification_code}}",
        "code: {{verification_code}}",
    ];
    for p in suffix_patterns {
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

    let mut lines = Vec::new();
    let mut skip_next_blank = false;

    for raw_line in template.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        if contains_verification_code_placeholder(line) {
            if is_standalone_verification_code_line(line) {
                skip_next_blank = true;
                continue;
            }
            skip_next_blank = false;
            lines.push(clean_inline_verification_code_placeholder(line));
        } else {
            if skip_next_blank && line.trim().is_empty() {
                continue;
            }
            skip_next_blank = false;
            lines.push(line.to_string());
        }
    }

    lines.join("\n")
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

    #[test]
    fn test_clean_custom_emoji_and_bracket_templates() {
        assert_eq!(
            clean_empty_verification_code_template("🔐 验证码: {{验证码}}"),
            ""
        );
        assert_eq!(
            clean_empty_verification_code_template(
                "⭐ 您的验证码是：【{{验证码}}】\n\n📝 {{短信内容}}\n\n"
            ),
            "📝 {{短信内容}}\n\n"
        );
        assert_eq!(
            clean_empty_verification_code_template(
                "号码: {{发送方号码}}, 验证码: {{验证码}}, 内容: {{短信内容}}"
            ),
            "号码: {{发送方号码}}, 内容: {{短信内容}}"
        );
        assert_eq!(
            clean_empty_verification_code_template(
                "号码: {{发送方号码}}，您的验证码是：【{{验证码}}】，内容: {{短信内容}}"
            ),
            "号码: {{发送方号码}}，内容: {{短信内容}}"
        );
    }
}
