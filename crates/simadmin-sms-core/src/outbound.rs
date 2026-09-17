#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsEncoding {
    Gsm7Bit,
    Ucs2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmsSegmentEstimate {
    pub encoding: SmsEncoding,
    pub char_count: usize,
    pub segment_count: usize,
    pub chars_per_segment: usize,
}

pub fn estimate_sms_segments(content: &str) -> SmsSegmentEstimate {
    let char_count = content.chars().count();
    let is_gsm7 = content.chars().all(|c| {
        c.is_ascii() || matches!(c, '€' | '£' | '¥' | '§' | '¿' | '¡')
    });

    let (encoding, max_single, max_concat) = if is_gsm7 {
        (SmsEncoding::Gsm7Bit, 160, 153)
    } else {
        (SmsEncoding::Ucs2, 70, 67)
    };

    let (segment_count, chars_per_segment) = if char_count <= max_single {
        (if char_count == 0 { 0 } else { 1 }, max_single)
    } else {
        let count = (char_count + max_concat - 1) / max_concat;
        (count, max_concat)
    };

    SmsSegmentEstimate {
        encoding,
        char_count,
        segment_count,
        chars_per_segment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate() {
        let est1 = estimate_sms_segments("Hello world");
        assert_eq!(est1.encoding, SmsEncoding::Gsm7Bit);
        assert_eq!(est1.segment_count, 1);

        let est2 = estimate_sms_segments("你好世界");
        assert_eq!(est2.encoding, SmsEncoding::Ucs2);
        assert_eq!(est2.segment_count, 1);

        let long_cn = "中".repeat(75);
        let est3 = estimate_sms_segments(&long_cn);
        assert_eq!(est3.encoding, SmsEncoding::Ucs2);
        assert_eq!(est3.segment_count, 2);
    }
}
