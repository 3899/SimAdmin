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
    "动态密码",
    "认证码",
    "短信码",
    "登录码",
    "安全码",
    "安全代码",
    "确认码",
    "验证代码",
    "一次性代码",
    "一次性验证码",
    "一次性密码",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateKind {
    Numeric,
    AlphaNumeric,
    UpperAlpha,
}

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
        if !chars[index].is_ascii_alphanumeric() {
            index += 1;
            continue;
        }

        let start = index;
        let Some((code, end, kind)) = parse_candidate_at(&chars, start) else {
            index = read_alnum_run(&chars, start).max(start + 1);
            continue;
        };

        let score = score_candidate(&chars, start, end, &code, kind);
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

fn parse_candidate_at(chars: &[char], start: usize) -> Option<(String, usize, CandidateKind)> {
    let first_end = read_alnum_run(chars, start);
    let first_len = first_end - start;
    if (MIN_CODE_LEN..=MAX_CODE_LEN).contains(&first_len) {
        let token = chars[start..first_end].iter().collect::<String>();
        return classify_candidate(&token).map(|kind| (token, first_end, kind));
    }

    let first_is_digits = chars[start..first_end].iter().all(|ch| ch.is_ascii_digit());
    if !first_is_digits
        || first_len < 2
        || first_end >= chars.len()
        || !is_code_digit_separator(chars[first_end])
        || first_end + 1 >= chars.len()
        || !chars[first_end + 1].is_ascii_digit()
    {
        return None;
    }

    let second_start = first_end + 1;
    let second_end = read_digit_run(chars, second_start);
    if second_end < chars.len() && chars[second_end].is_ascii_alphanumeric() {
        return None;
    }
    let second_len = second_end - second_start;
    let total_len = first_len + second_len;
    if second_len < 2 || !(MIN_CODE_LEN..=MAX_CODE_LEN).contains(&total_len) {
        return None;
    }

    let mut code = chars[start..first_end].iter().collect::<String>();
    code.extend(chars[second_start..second_end].iter().copied());
    Some((code, second_end, CandidateKind::Numeric))
}

fn read_digit_run(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn read_alnum_run(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_alphanumeric() {
        end += 1;
    }
    end
}

fn classify_candidate(token: &str) -> Option<CandidateKind> {
    let has_digit = token.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = token.bytes().any(|b| b.is_ascii_alphabetic());

    if has_digit && !has_alpha {
        return Some(CandidateKind::Numeric);
    }
    if has_digit && has_alpha {
        if is_digit_unit_token(token) || looks_like_word_with_digits(token) {
            return None;
        }
        return Some(CandidateKind::AlphaNumeric);
    }
    if !has_digit && token.bytes().all(|b| b.is_ascii_uppercase()) {
        if NON_CODE_UPPER_WORDS.contains(&token) {
            return None;
        }
        return Some(CandidateKind::UpperAlpha);
    }
    None
}

fn is_digit_unit_token(token: &str) -> bool {
    let digit_len = token.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digit_len == 0 || digit_len == token.len() {
        return false;
    }
    let suffix = token[digit_len..].to_ascii_lowercase();
    matches!(
        suffix.as_str(),
        "gb" | "mb"
            | "kb"
            | "tb"
            | "pb"
            | "g"
            | "m"
            | "k"
            | "t"
            | "bps"
            | "kbps"
            | "mbps"
            | "gbps"
            | "hz"
            | "khz"
            | "mhz"
            | "ghz"
            | "mah"
            | "kg"
            | "km"
            | "cm"
            | "mm"
            | "ms"
            | "px"
    )
}

fn looks_like_word_with_digits(token: &str) -> bool {
    let bytes = token.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let lower_run = bytes[1..]
        .iter()
        .take_while(|b| b.is_ascii_lowercase())
        .count();
    let rest = &bytes[1 + lower_run..];
    lower_run >= 2 && !rest.is_empty() && rest.iter().all(|b| b.is_ascii_digit())
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

fn score_candidate(
    chars: &[char],
    start: usize,
    end: usize,
    code: &str,
    kind: CandidateKind,
) -> i32 {
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

    // 非纯数字候选必须处于强关键词紧邻上下文中，防止普通英文单词或 URL 哈希被误提取
    match kind {
        CandidateKind::Numeric => {}
        CandidateKind::AlphaNumeric => {
            if !strong_immediate {
                return 0;
            }
        }
        CandidateKind::UpperAlpha => {
            if !cjk_strong_keyword_touches(&before, &after) {
                return 0;
            }
        }
    }

    let mut score = match code.chars().count() {
        6 => 60,
        4 => 42,
        5 => 24,
        7 => 22,
        8 => 20,
        _ => 0,
    };

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
    if negative_keyword_touches_candidate(&before)
        || negative_suffix_touches_candidate(&after)
        || is_in_url_or_domain(&before, &after)
    {
        score -= 80;
    }
    if !strong_immediate && SERVICE_HOTLINES.contains(&code) {
        score -= 80;
    }
    if contains_any(&near, NEGATIVE_KEYWORDS) {
        score -= if strong_immediate { 5 } else { 35 };
    }

    score
}

const NON_CODE_UPPER_WORDS: &[&str] = &[
    "CODE", "HTTP", "HTTPS", "WIFI", "VOWIFI", "VOLTE", "USDT", "CNY", "RMB", "FREE", "TEST",
    "NONE", "NULL", "TRUE", "FALSE", "BANK", "PASS", "USER", "SMS",
];

const NEGATIVE_SUFFIXES: &[&str] = &[
    "年", "月", "日", "号", "时", "分", "秒", "点", "元", "角", "分", "块", "次", "折", "%",
    "gb", "mb", "kb", "tb", "g", "m", "k", "t", "bps",
];

const SERVICE_HOTLINES: &[&str] = &[
    "10086", "10010", "10000", "10001", "12306", "12315", "12345", "95588", "95533", "95566",
    "95599", "95555", "95559", "95511",
];

fn negative_suffix_touches_candidate(after: &str) -> bool {
    let trimmed = after.trim_start();
    NEGATIVE_SUFFIXES.iter().any(|suffix| {
        if suffix.is_ascii() {
            trimmed.to_ascii_lowercase().starts_with(suffix)
        } else {
            trimmed.starts_with(suffix)
        }
    })
}

fn is_in_url_or_domain(before: &str, after: &str) -> bool {
    before.ends_with('.')
        || after.starts_with('.')
        || before.ends_with('/')
        || after.starts_with('/')
        || before.ends_with('=')
        || before.ends_with('?')
        || before.ends_with('&')
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
        || contains_ascii_phrase(value, "security code")
        || contains_ascii_phrase(value, "one-time code")
        || contains_ascii_phrase(value, "one time code")
        || contains_ascii_phrase(value, "one-time password")
        || contains_ascii_word(value, "code")
        || contains_ascii_word(value, "otp")
        || contains_ascii_word(value, "pin")
        || contains_ascii_word(value, "passcode")
}

fn cjk_strong_keyword_touches(before: &str, after: &str) -> bool {
    let before = before.trim_end_matches(is_connector);
    let after = after.trim_start_matches(is_connector);
    STRONG_KEYWORDS
        .iter()
        .any(|keyword| before.ends_with(keyword) || after.starts_with(keyword))
}

fn keyword_touches_candidate(before: &str, after: &str) -> bool {
    cjk_strong_keyword_touches(before, after)
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
            | '\n'
            | '\r'
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
            | '['
            | ']'
            | '【'
            | '】'
            | '"'
            | '\''
            | '“'
            | '”'
            | '`'
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
            ("【Microsoft】Microsoft 帐户一次性代码: 202423", "202423"),
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
            "【流量周提醒】尊敬的客户，您好！截至09月29日11时32分，您本月移动数据流量已使用0.00MB，国内通用流量剩余0.00MB，定向流量剩余200.00GB。具体以月结账单为准。查询详情登录中国移动APP请点击 https://dx.10086.cn/A/5sstHA。上中国移动APP，查看网龄权益，至高3GB通用月包流量、热门视频会员等，请点击 https://dx.10086.cn/A/ceMnEA。我们百倍努力，只为您10分满意【中国移动】",
            "尊敬的客户,截止2026年10月04日00时01分您的当前账户余额是25.70元。您还可以关注微信公众号“上海电信”、登录中国电信网站(www.189.cn)或“中国电信”APP(http://a.189.cn/ulymab) 查询使用规则等详情。[中国电信]",
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

    #[test]
    fn extracts_alphanumeric_and_uppercase_codes() {
        let cases = [
            ("您正在进行登录验证，验证码为7VRU", "7VRU"),
            ("您的验证码是：【7VRU】", "7VRU"),
            ("【某平台】验证码：A1B2C3，5分钟内有效", "A1B2C3"),
            ("7VRU为您的登录验证码", "7VRU"),
            ("7VRU(登录验证码)", "7VRU"),
            ("Your verification code is A1B2", "A1B2"),
            ("OTP: 9K2M3P", "9K2M3P"),
            ("【某平台】验证码为ABCD，请勿泄露", "ABCD"),
            ("ABCD为您的动态验证码", "ABCD"),
            ("【某服务】确认码：8X9Y", "8X9Y"),
        ];

        for (content, expected) in cases {
            assert_eq!(
                extract_verification_code(content).as_deref(),
                Some(expected),
                "failed for: {content}"
            );
        }
    }

    #[test]
    fn does_not_extract_false_positive_alphanumeric_tokens() {
        let cases = [
            "Your code is valid for 5 minutes",
            "Don't forward the code to anyone",
            "Please check https://example.com?code=5sstHA for details",
            "您的套餐包含200GB流量与100MB免费额度",
            "Use code in App123 to login",
            "您的验证码已发送，请注意查收 WIFI 通知",
            "订单Order123已生成，请核对",
        ];

        for content in cases {
            assert_eq!(
                extract_verification_code(content),
                None,
                "unexpected match for: {content}"
            );
        }
    }
}
