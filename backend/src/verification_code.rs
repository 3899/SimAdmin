//! 验证码提取模块（向后兼容层）
//!
//! 核心算法已下沉至 `simadmin-sms-core` 纯业务共享库，此处透明重导出以保障内部兼容。

pub use simadmin_sms_core::verification_code::*;

#[cfg(test)]
mod tests {
    use super::extract_verification_code;

    #[test]
    fn extracts_common_verification_code_formats() {
        let cases = [
            ("验证码 1837", "1837"),
            ("验证码:798236", "798236"),
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
