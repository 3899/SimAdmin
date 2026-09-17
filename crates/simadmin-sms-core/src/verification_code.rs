const MIN_CODE_LEN: usize = 4;
const MAX_CODE_LEN: usize = 8;
const SCORE_THRESHOLD: i32 = 70;
const CONTEXT_WINDOW: usize = 32;
const IMMEDIATE_WINDOW: usize = 10;

const STRONG_KEYWORDS: &[&str] = &[
    "验证码",
    "校验码",
    "动态码",
    "动态验证码",
    "认证码",
    "短信码",
    "登录码",
    "安全码",
    "验证代码",
];

const ACTION_KEYWORDS: &[&str] = &[
    "登录", "注册", "绑定", "换绑", "重置", "密码", "支付", "验证", "申请",
];

const TRUST_KEYWORDS: &[&str] = &[
    "分钟内有效",
    "有效期",
    "请勿泄露",
    "不要告诉",
    "非本人",
    "本人操作",
    "完成验证",
];

const NEGATIVE_KEYWORDS: &[&str] = &[
    "订单号",
    "运单号",
    "快递单号",
    "工单号",
    "流水号",
    "编号",
    "序号",
    "金额",
    "余额",
    "积分",
    "卡号",
    "尾号",
    "手机号",
    "账号",
    "账户",
    "日期",
    "版本",
    "端口",
];

#[derive(Debug, Clone)]
struct Candidate {
    code: String,
    start: usize,
    score: i32,
}

pub fn extract_verification_code(content: &str) -> Option<String> {
    let normalized = normalize_digits(content);
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut best: Option<Candidate> = None;
    let mut index = 0usize;

    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let start = index;
        let Some((code, end)) = parse_candidate_at(&chars, start) else {
            index = read_digit_run(&chars, start);
            continue;
        };

        let score = score_candidate(&chars, start, end, code.chars().count());
        if score < SCORE_THRESHOLD {
            index = end;
            continue;
        }

        let candidate = Candidate { code, start, score };
        if best
            .as_ref()
            .map(|current| candidate_rank(&candidate) > candidate_rank(current))
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
        index = end;
    }

    best.map(|candidate| candidate.code)
}

fn parse_candidate_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let first_end = read_digit_run(chars, start);
    let first_len = first_end - start;
    if (MIN_CODE_LEN..=MAX_CODE_LEN).contains(&first_len) {
        return Some((chars[start..first_end].iter().collect(), first_end));
    }

    if first_len < 2
        || first_end >= chars.len()
        || !is_code_digit_separator(chars[first_end])
        || first_end + 1 >= chars.len()
        || !chars[first_end + 1].is_ascii_digit()
    {
        return None;
    }

    let second_start = first_end + 1;
    let second_end = read_digit_run(chars, second_start);
    let second_len = second_end - second_start;
    let total_len = first_len + second_len;
    if second_len < 2 || !(MIN_CODE_LEN..=MAX_CODE_LEN).contains(&total_len) {
        return None;
    }

    let mut code = chars[start..first_end].iter().collect::<String>();
    code.extend(chars[second_start..second_end].iter().copied());
    Some((code, second_end))
}

fn read_digit_run(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn normalize_digits(content: &str) -> String {
    content
        .chars()
        .map(|ch| match ch {
            '０'..='９' => char::from_u32('0' as u32 + ch as u32 - '０' as u32).unwrap_or(ch),
            _ => ch,
        })
        .collect()
}

fn score_candidate(chars: &[char], start: usize, end: usize, len: usize) -> i32 {
    let mut score = match len {
        6 => 60,
        4 => 42,
        5 => 24,
        7 => 22,
        8 => 20,
        _ => 0,
    };

    let near = slice(
        chars,
        start.saturating_sub(CONTEXT_WINDOW),
        (end + CONTEXT_WINDOW).min(chars.len()),
    );
    let immediate = slice(
        chars,
        start.saturating_sub(IMMEDIATE_WINDOW),
        (end + IMMEDIATE_WINDOW).min(chars.len()),
    );
    let before = slice(chars, start.saturating_sub(IMMEDIATE_WINDOW), start);
    let after = slice(chars, end, (end + IMMEDIATE_WINDOW).min(chars.len()));

    let strong_near = contains_strong_keyword(&near);
    let strong_immediate = contains_strong_keyword(&immediate);
    if strong_near {
        score += 35;
    }
    if strong_immediate {
        score += 20;
    }
    if contains_any(&near, ACTION_KEYWORDS) {
        score += 10;
    }
    if contains_any(&near, TRUST_KEYWORDS) {
        score += 10;
    }
    if keyword_touches_candidate(&before, &after) {
        score += 15;
    }
    if negative_keyword_touches_candidate(&before) {
        score -= 80;
    }
    if contains_any(&near, NEGATIVE_KEYWORDS) {
        score -= if strong_immediate { 5 } else { 35 };
    }

    score
}

fn candidate_rank(candidate: &Candidate) -> (i32, i32, std::cmp::Reverse<usize>) {
    (
        candidate.score,
        length_priority(candidate.code.chars().count()),
        std::cmp::Reverse(candidate.start),
    )
}

fn length_priority(len: usize) -> i32 {
    match len {
        6 => 5,
        4 => 4,
        5 => 3,
        7 => 2,
        8 => 1,
        _ => 0,
    }
}

fn slice(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}

fn contains_any(value: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| value.contains(keyword))
}

fn contains_strong_keyword(value: &str) -> bool {
    contains_any(value, STRONG_KEYWORDS)
        || contains_ascii_phrase(value, "verification code")
        || contains_ascii_phrase(value, "verify code")
        || contains_ascii_word(value, "code")
        || contains_ascii_word(value, "otp")
        || contains_ascii_word(value, "pin")
        || contains_ascii_word(value, "passcode")
}

fn keyword_touches_candidate(before: &str, after: &str) -> bool {
    let before = before.trim_end_matches(is_connector);
    let after = after.trim_start_matches(is_connector);
    STRONG_KEYWORDS
        .iter()
        .any(|keyword| before.ends_with(keyword) || after.starts_with(keyword))
        || contains_ascii_word(before, "code")
        || contains_ascii_word(after, "code")
}

fn negative_keyword_touches_candidate(before: &str) -> bool {
    let before = before.trim_end_matches(is_connector);
    !contains_strong_keyword(before)
        && NEGATIVE_KEYWORDS
            .iter()
            .any(|keyword| before.ends_with(keyword))
}

fn is_code_digit_separator(ch: char) -> bool {
    matches!(ch, '-' | '－' | '‐' | '‑' | '–')
}

fn is_connector(ch: char) -> bool {
    matches!(
        ch,
        ':' | '：'
            | ' '
            | '\t'
            | '-'
            | '_'
            | '是'
            | '为'
            | '的'
            | '您'
            | '你'
            | '，'
            | ','
            | '('
            | ')'
            | '（'
            | '）'
    )
}

fn contains_ascii_phrase(value: &str, phrase: &str) -> bool {
    value.to_ascii_lowercase().contains(phrase)
}

fn contains_ascii_word(value: &str, word: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.match_indices(word).any(|(index, _)| {
        let before = lower[..index].chars().next_back();
        let after = lower[index + word.len()..].chars().next();
        !before.map(is_ascii_word_char).unwrap_or(false)
            && !after.map(is_ascii_word_char).unwrap_or(false)
    })
}

fn is_ascii_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::extract_verification_code;

    #[test]
    fn extracts_common_verification_code_formats() {
        let cases = [
            ("验证码1837", "1837"),
            ("验证码798236", "798236"),
            ("【谷歌信息】G-248521是您的 Google 验证码", "248521"),
            ("【大众点评】170426 (登录验证码，请完成验证)", "170426"),
            ("【哔哩哔哩】257707为你的手机换绑验证码", "257707"),
            ("【网上国网】804306，您申请的网上国网验证码。", "804306"),
            ("【美团】649181 (绑定手机验证码)", "649181"),
            ("【建设银行】序号01的验证码089053", "089053"),
            ("722335(动态验证码)", "722335"),
            (
                "[WeChat] Your Weixin is linking or verifying mobile number (035273). Don't forward the code!",
                "035273",
            ),
            ("Telegram code: 25322", "25322"),
            (
                "[抖音] 2461 is your verification code, valid for 5 minutes.",
                "2461",
            ),
            ("您的 WhatsApp 验证码: 161-675", "161675"),
        ];

        for (content, expected) in cases {
            assert_eq!(
                extract_verification_code(content).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn extracts_ascii_and_fullwidth_digit_codes() {
        assert_eq!(
            extract_verification_code("Your code is 482910").as_deref(),
            Some("482910")
        );
        assert_eq!(
            extract_verification_code("验证码：１２３４５６").as_deref(),
            Some("123456")
        );
    }

    #[test]
    fn does_not_extract_unqualified_numbers() {
        let cases = [
            "您的订单号123456已发货",
            "余额123456元已到账",
            "您的订单号123-456已发货",
            "手机号13800138000登录成功",
            "今天温度1234，湿度5678",
        ];

        for content in cases {
            assert_eq!(extract_verification_code(content), None);
        }
    }

    #[test]
    fn prefers_highest_scored_candidate() {
        assert_eq!(
            extract_verification_code("订单号123456，验证码654321").as_deref(),
            Some("654321")
        );
    }
}
