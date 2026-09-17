use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmsPartHeader {
    pub reference_number: u16,
    pub total_parts: u8,
    pub part_sequence: u8,
}

#[derive(Debug, Clone)]
pub struct SmsPartInput {
    pub device_id: String,
    pub sender: String,
    pub timestamp_rfc3339: String,
    pub content: String,
    pub pdu: Option<String>,
    pub header: Option<SmsPartHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledSms {
    pub device_id: String,
    pub sender: String,
    pub timestamp_rfc3339: String,
    pub content: String,
    pub pdu: Option<String>,
    pub content_hash: String,
    pub pdu_hash: String,
    pub part_count: u8,
    pub is_partial_fallback: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AssemblyResult {
    Pending { current: u8, total: u8 },
    Complete(Box<AssembledSms>),
}

struct AssemblySession {
    device_id: String,
    sender: String,
    timestamp_rfc3339: String,
    total_parts: u8,
    parts: BTreeMap<u8, (String, Option<String>)>, // sequence -> (content, pdu)
    created_at: Instant,
}

pub struct SmsAssembler {
    sessions: HashMap<String, AssemblySession>,
    ttl: Duration,
}

impl SmsAssembler {
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            ttl,
        }
    }

    /// 解析二进制 PDU 内部的 3GPP TS 23.040 UDH
    pub fn parse_udh_header(pdu_bytes: &[u8]) -> Option<SmsPartHeader> {
        if pdu_bytes.len() < 7 {
            return None;
        }
        let mut idx = 0;
        while idx < pdu_bytes.len() {
            let iei = pdu_bytes[idx];
            if idx + 1 >= pdu_bytes.len() {
                break;
            }
            let ie_len = pdu_bytes[idx + 1] as usize;
            let data_idx = idx + 2;
            if data_idx + ie_len > pdu_bytes.len() {
                break;
            }
            // 8-bit concatenated SMS
            if iei == 0x00 && ie_len == 3 {
                return Some(SmsPartHeader {
                    reference_number: pdu_bytes[data_idx] as u16,
                    total_parts: pdu_bytes[data_idx + 1],
                    part_sequence: pdu_bytes[data_idx + 2],
                });
            }
            // 16-bit concatenated SMS
            if iei == 0x08 && ie_len == 4 {
                let ref_num = u16::from_be_bytes([pdu_bytes[data_idx], pdu_bytes[data_idx + 1]]);
                return Some(SmsPartHeader {
                    reference_number: ref_num,
                    total_parts: pdu_bytes[data_idx + 2],
                    part_sequence: pdu_bytes[data_idx + 3],
                });
            }
            idx = data_idx + ie_len;
        }
        None
    }

    /// 从短信正文提取文本分片标记，如 `(1/3)`, `[2/2]`, `【1/2】`, `（1/3）`
    pub fn parse_text_header(content: &str) -> Option<(SmsPartHeader, String)> {
        let trimmed = content.trim();
        let prefixes = [('(', ')'), ('[', ']'), ('\u{3010}', '\u{3011}'), ('\u{ff08}', '\u{ff09}')];
        for (start_ch, end_ch) in prefixes {
            if trimmed.starts_with(start_ch) {
                if let Some(end_idx) = trimmed.find(end_ch) {
                    let tag = &trimmed[start_ch.len_utf8()..end_idx];
                    if let Some((seq_str, total_str)) = tag.split_once('/') {
                        if let (Ok(seq), Ok(total)) = (seq_str.trim().parse::<u8>(), total_str.trim().parse::<u8>()) {
                            if total > 1 && seq >= 1 && seq <= total {
                                let rem = trimmed[end_idx + end_ch.len_utf8()..].trim().to_string();
                                return Some((
                                    SmsPartHeader {
                                        reference_number: 0,
                                        total_parts: total,
                                        part_sequence: seq,
                                    },
                                    rem,
                                ));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn ingest_part(&mut self, mut part: SmsPartInput, now: Instant) -> AssemblyResult {
        // 若没有明确 header，尝试文本解析
        if part.header.is_none() {
            if let Some((header, stripped)) = Self::parse_text_header(&part.content) {
                part.header = Some(header);
                part.content = stripped;
            }
        }

        let Some(header) = part.header else {
            // 单条独立短信直接返回 Complete
            let content_hash = crate::fingerprint::compute_content_hash(&part.content);
            let pdu_hash = crate::fingerprint::compute_pdu_hash(part.pdu.as_deref());
            return AssemblyResult::Complete(Box::new(AssembledSms {
                device_id: part.device_id,
                sender: part.sender,
                timestamp_rfc3339: part.timestamp_rfc3339,
                content: part.content,
                pdu: part.pdu,
                content_hash,
                pdu_hash,
                part_count: 1,
                is_partial_fallback: false,
            }));
        };

        let session_key = format!("{}:{}:{}", part.device_id, part.sender, header.reference_number);
        let session = self.sessions.entry(session_key.clone()).or_insert_with(|| AssemblySession {
            device_id: part.device_id.clone(),
            sender: part.sender.clone(),
            timestamp_rfc3339: part.timestamp_rfc3339.clone(),
            total_parts: header.total_parts,
            parts: BTreeMap::new(),
            created_at: now,
        });

        session.parts.insert(header.part_sequence, (part.content, part.pdu));

        if session.parts.len() >= session.total_parts as usize {
            let session = self.sessions.remove(&session_key).unwrap();
            let mut full_content = String::new();
            let mut last_pdu = None;
            for (_seq, (content, pdu)) in session.parts {
                full_content.push_str(&content);
                if pdu.is_some() {
                    last_pdu = pdu;
                }
            }
            let content_hash = crate::fingerprint::compute_content_hash(&full_content);
            let pdu_hash = crate::fingerprint::compute_pdu_hash(last_pdu.as_deref());
            AssemblyResult::Complete(Box::new(AssembledSms {
                device_id: session.device_id,
                sender: session.sender,
                timestamp_rfc3339: session.timestamp_rfc3339,
                content: full_content,
                pdu: last_pdu,
                content_hash,
                pdu_hash,
                part_count: session.total_parts,
                is_partial_fallback: false,
            }))
        } else {
            AssemblyResult::Pending {
                current: session.parts.len() as u8,
                total: session.total_parts,
            }
        }
    }

    pub fn purge_expired(&mut self, now: Instant) -> Vec<AssembledSms> {
        let mut expired_keys = Vec::new();
        for (key, session) in &self.sessions {
            if now.saturating_duration_since(session.created_at) > self.ttl {
                expired_keys.push(key.clone());
            }
        }

        let mut results = Vec::new();
        for key in expired_keys {
            if let Some(session) = self.sessions.remove(&key) {
                let mut full_content = String::new();
                let mut last_pdu = None;
                let part_count = session.parts.len() as u8;
                for (_seq, (content, pdu)) in session.parts {
                    full_content.push_str(&content);
                    if pdu.is_some() {
                        last_pdu = pdu;
                    }
                }
                let content_hash = crate::fingerprint::compute_content_hash(&full_content);
                let pdu_hash = crate::fingerprint::compute_pdu_hash(last_pdu.as_deref());
                results.push(AssembledSms {
                    device_id: session.device_id,
                    sender: session.sender,
                    timestamp_rfc3339: session.timestamp_rfc3339,
                    content: full_content,
                    pdu: last_pdu,
                    content_hash,
                    pdu_hash,
                    part_count,
                    is_partial_fallback: true,
                });
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_concatenation() {
        let mut assembler = SmsAssembler::new(Duration::from_secs(60));
        let now = Instant::now();

        let part1 = SmsPartInput {
            device_id: "dev-1".into(),
            sender: "10086".into(),
            timestamp_rfc3339: "2026-09-17T12:00:00Z".into(),
            content: "[1/2]您的验证码是".into(),
            pdu: None,
            header: None,
        };
        let part2 = SmsPartInput {
            device_id: "dev-1".into(),
            sender: "10086".into(),
            timestamp_rfc3339: "2026-09-17T12:00:01Z".into(),
            content: "[2/2]842910，请勿泄露".into(),
            pdu: None,
            header: None,
        };

        let res1 = assembler.ingest_part(part1, now);
        assert_eq!(res1, AssemblyResult::Pending { current: 1, total: 2 });

        let res2 = assembler.ingest_part(part2, now);
        match res2 {
            AssemblyResult::Complete(assembled) => {
                assert_eq!(assembled.content, "您的验证码是842910，请勿泄露");
                assert_eq!(assembled.part_count, 2);
                assert!(!assembled.is_partial_fallback);
            }
            _ => panic!("Expected Complete"),
        }
    }

    #[test]
    fn test_purge_expired() {
        let mut assembler = SmsAssembler::new(Duration::from_millis(50));
        let now = Instant::now();
        let part1 = SmsPartInput {
            device_id: "dev-1".into(),
            sender: "10086".into(),
            timestamp_rfc3339: "2026-09-17T12:00:00Z".into(),
            content: "(1/2) Lonely segment".into(),
            pdu: None,
            header: None,
        };
        assembler.ingest_part(part1, now);
        let future = now + Duration::from_millis(100);
        let expired = assembler.purge_expired(future);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].content, "Lonely segment");
        assert!(expired[0].is_partial_fallback);
    }
}
