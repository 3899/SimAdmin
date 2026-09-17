use sha2::{Digest, Sha256};

/// 纯文本内容 SHA-256 哈希
pub fn compute_content_hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// 原始 PDU SHA-256 哈希（当 pdu 为 None 或空串时，统一计算空串的标准 SHA-256）
pub fn compute_pdu_hash(pdu: Option<&str>) -> String {
    let raw = pdu.unwrap_or("");
    hex::encode(Sha256::digest(raw.as_bytes()))
}

/// 设备内确定性唯一幂等指纹
pub fn compute_sms_fingerprint(
    device_id: &str,
    phone_number: &str,
    timestamp_rfc3339: &str,
    content_hash: &str,
    pdu_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(phone_number.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(timestamp_rfc3339.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(content_hash.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(pdu_hash.trim().as_bytes());
    format!("smfp:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hashes() {
        let hash = compute_content_hash("hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        let empty_pdu_hash = compute_pdu_hash(None);
        assert_eq!(
            empty_pdu_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(compute_pdu_hash(Some("")), empty_pdu_hash);
    }

    #[test]
    fn test_compute_fingerprint() {
        let fp1 = compute_sms_fingerprint("dev-1", "13800000000", "2026-09-17T12:00:00Z", "hash1", "hash2");
        let fp2 = compute_sms_fingerprint("dev-1", "13800000000", "2026-09-17T12:00:00Z", "hash1", "hash2");
        let fp3 = compute_sms_fingerprint("dev-2", "13800000000", "2026-09-17T12:00:00Z", "hash1", "hash2");
        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3);
        assert!(fp1.starts_with("smfp:"));
    }
}
