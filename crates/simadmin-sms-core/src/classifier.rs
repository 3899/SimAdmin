use crate::types::SmsCategory;

const MARKETING_KEYWORDS: &[&str] = &[
    "退订回", "退订请", "回复TD", "回复T", "回T退订", "回TD退订", "回N退订",
    "拒收请回复", "关注公众号", "戳链接", "点击链接", "领取优惠券", "立减",
    "限时特惠", "开门红", "秒杀", "首充",
];

const NOTIFICATION_KEYWORDS: &[&str] = &[
    "尾号", "存入", "支出", "动账", "余额", "扣款", "快递", "包裹", "取件码",
    "派送", "已签收", "顺丰", "菜鸟", "驿站", "京东物流", "订单已发货", "运单号",
];

pub fn classify_sms(content: &str) -> SmsCategory {
    // 1. 若能加权提取到高分验证码，归类为 VerificationCode
    if crate::verification_code::extract_verification_code(content).is_some() {
        return SmsCategory::VerificationCode;
    }

    // 2. 营销识别
    for kw in MARKETING_KEYWORDS {
        if content.contains(kw) {
            return SmsCategory::Marketing;
        }
    }

    // 3. 物流/动账通知
    for kw in NOTIFICATION_KEYWORDS {
        if content.contains(kw) {
            return SmsCategory::Notification;
        }
    }

    SmsCategory::Personal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify() {
        assert_eq!(
            classify_sms("【中国银行】您的验证码是 482910，请勿泄露。"),
            SmsCategory::VerificationCode
        );
        assert_eq!(
            classify_sms("【某商场】双11大促全场5折，戳链接抢购，退订回T"),
            SmsCategory::Marketing
        );
        assert_eq!(
            classify_sms("【菜鸟驿站】您的包裹已到达，取件码 5-2-3001。"),
            SmsCategory::Notification
        );
        assert_eq!(
            classify_sms("今天晚上一起吃饭吗？"),
            SmsCategory::Personal
        );
    }
}
