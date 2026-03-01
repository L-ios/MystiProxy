//! NTLM 认证模块
//!
//! 提供 NTLM/NTLMv2 认证支持，类似 Cntlm
//!
//! NTLM 是一种挑战-响应认证协议，常用于企业代理服务器

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use des::cipher::{BlockEncrypt, KeyInit};
use des::Des;
use hmac::{Hmac, Mac};
use md4::{Digest, Md4};
use md5::Md5;
use rand::thread_rng;
use rand::RngCore;
use tracing::debug;

use crate::context::thread_identity;
use crate::error::{MystiProxyError, Result};

macro_rules! log_debug {
    ($($arg:tt)*) => {
        debug!("[{}] {}", thread_identity(), format!($($arg)*))
    };
}

/// NTLM 认证类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtlmVersion {
    /// NTLMv1
    V1,
    /// NTLMv2 (推荐)
    V2,
}

/// NTLM 认证配置
#[derive(Debug, Clone)]
pub struct NtlmConfig {
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 域名 (可选)
    pub domain: String,
    /// 工作站名 (可选)
    pub workstation: String,
    /// NTLM 版本
    pub version: NtlmVersion,
}

impl NtlmConfig {
    /// 创建新的 NTLM 配置
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
            domain: String::new(),
            workstation: String::new(),
            version: NtlmVersion::V2,
        }
    }

    /// 设置域名
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = domain.into();
        self
    }

    /// 设置工作站名
    pub fn workstation(mut self, workstation: impl Into<String>) -> Self {
        self.workstation = workstation.into();
        self
    }

    /// 设置 NTLM 版本
    pub fn version(mut self, version: NtlmVersion) -> Self {
        self.version = version;
        self
    }
}

/// NTLM 认证器
pub struct NtlmAuthenticator {
    config: NtlmConfig,
}

impl NtlmAuthenticator {
    /// 创建新的 NTLM 认证器
    pub fn new(config: NtlmConfig) -> Self {
        Self { config }
    }

    /// 生成 Type 1 消息 (Negotiate)
    pub fn create_type1_message(&self) -> String {
        let domain = self.config.domain.as_bytes();
        let workstation = self.config.workstation.as_bytes();

        let domain_len = domain.len() as u16;
        let workstation_len = workstation.len() as u16;

        let mut message = Vec::new();

        // Signature
        message.extend_from_slice(b"NTLMSSP\x00");

        // Message Type (1)
        message.extend_from_slice(&1u32.to_le_bytes());

        // Flags
        let flags = NTLM_FLAGS_NEGOTIATE_UNICODE
            | NTLM_FLAGS_NEGOTIATE_OEM
            | NTLM_FLAGS_REQUEST_TARGET
            | NTLM_FLAGS_NEGOTIATE_NTLM
            | NTLM_FLAGS_NEGOTIATE_ALWAYS_SIGN
            | NTLM_FLAGS_NEGOTIATE_EXTENDED_SESSIONSECURITY
            | NTLM_FLAGS_NEGOTIATE_VERSION
            | NTLM_FLAGS_NEGOTIATE_128
            | NTLM_FLAGS_NEGOTIATE_KEY_EXCH;
        message.extend_from_slice(&flags.to_le_bytes());

        // Domain (empty for Type 1)
        message.extend_from_slice(&domain_len.to_le_bytes());
        message.extend_from_slice(&domain_len.to_le_bytes());
        message.extend_from_slice(&32u32.to_le_bytes());

        // Workstation (empty for Type 1)
        message.extend_from_slice(&workstation_len.to_le_bytes());
        message.extend_from_slice(&workstation_len.to_le_bytes());
        let workstation_offset = 32u32 + domain_len as u32;
        message.extend_from_slice(&workstation_offset.to_le_bytes());

        // Version (Windows 10)
        message.extend_from_slice(&NTLM_VERSION);

        // Padding
        message.extend_from_slice(&[0u8; 8]);

        // Domain data
        if !domain.is_empty() {
            message.extend_from_slice(domain);
        }

        // Workstation data
        if !workstation.is_empty() {
            message.extend_from_slice(workstation);
        }

        BASE64.encode(&message)
    }

    /// 解析 Type 2 消息 (Challenge)
    pub fn parse_type2_message(&self, message: &str) -> Result<Type2Message> {
        let decoded = BASE64
            .decode(message.trim_start_matches("NTLM "))
            .map_err(|e| MystiProxyError::Proxy(format!("Invalid NTLM Type2 message: {}", e)))?;

        if decoded.len() < 48 {
            return Err(MystiProxyError::Proxy("NTLM Type2 message too short".to_string()));
        }

        // Verify signature
        if &decoded[0..8] != b"NTLMSSP\x00" {
            return Err(MystiProxyError::Proxy("Invalid NTLM signature".to_string()));
        }

        // Verify message type (2)
        let msg_type = u32::from_le_bytes([decoded[8], decoded[9], decoded[10], decoded[11]]);
        if msg_type != 2 {
            return Err(MystiProxyError::Proxy(format!("Expected Type 2, got {}", msg_type)));
        }

        // Extract challenge
        let challenge = [decoded[24], decoded[25], decoded[26], decoded[27], decoded[28], decoded[29], decoded[30], decoded[31]];

        // Extract target info
        let target_info_len = u16::from_le_bytes([decoded[40], decoded[41]]) as usize;
        let target_info_offset = u32::from_le_bytes([decoded[44], decoded[45], decoded[46], decoded[47]]) as usize;

        let target_info = if target_info_len > 0 && target_info_offset + target_info_len <= decoded.len() {
            decoded[target_info_offset..target_info_offset + target_info_len].to_vec()
        } else {
            Vec::new()
        };

        log_debug!("Parsed NTLM Type2 message, challenge: {:02x?}", challenge);

        Ok(Type2Message {
            challenge,
            target_info,
            flags: u32::from_le_bytes([decoded[20], decoded[21], decoded[22], decoded[23]]),
        })
    }

    /// 生成 Type 3 消息 (Authenticate)
    pub fn create_type3_message(&self, type2: &Type2Message) -> String {
        match self.config.version {
            NtlmVersion::V1 => self.create_type3_message_v1(type2),
            NtlmVersion::V2 => self.create_type3_message_v2(type2),
        }
    }

    /// 生成 NTLMv1 Type 3 消息
    fn create_type3_message_v1(&self, type2: &Type2Message) -> String {
        let ntlm_hash = compute_ntlm_hash(&self.config.password);
        let nt_response = compute_ntlm_response_v1(&ntlm_hash, &type2.challenge);

        self.build_type3_message(&nt_response, &[])
    }

    /// 生成 NTLMv2 Type 3 消息
    fn create_type3_message_v2(&self, type2: &Type2Message) -> String {
        let ntlm_hash = compute_ntlm_hash(&self.config.password);
        let nt_response = compute_ntlm_response_v2(
            &ntlm_hash,
            &self.config.username,
            &self.config.domain,
            &type2.challenge,
            &type2.target_info,
        );

        self.build_type3_message(&nt_response, &[])
    }

    /// 构建 Type 3 消息
    fn build_type3_message(&self, nt_response: &[u8], _nt_proof_str: &[u8]) -> String {
        let domain = self.config.domain.as_bytes();
        let username = self.config.username.as_bytes();
        let workstation = self.config.workstation.as_bytes();

        let domain_len = domain.len() as u16;
        let username_len = username.len() as u16;
        let workstation_len = workstation.len() as u16;
        let nt_response_len = nt_response.len() as u16;

        let mut message = Vec::new();

        // Signature
        message.extend_from_slice(b"NTLMSSP\x00");

        // Message Type (3)
        message.extend_from_slice(&3u32.to_le_bytes());

        // LM Response (8 zeros for NTLMv2)
        let lm_response_offset = 64u32;
        message.extend_from_slice(&8u16.to_le_bytes()); // Len
        message.extend_from_slice(&8u16.to_le_bytes()); // Max Len
        message.extend_from_slice(&lm_response_offset.to_le_bytes()); // Offset

        // NT Response
        let nt_response_offset = lm_response_offset + 8;
        message.extend_from_slice(&nt_response_len.to_le_bytes());
        message.extend_from_slice(&nt_response_len.to_le_bytes());
        message.extend_from_slice(&nt_response_offset.to_le_bytes());

        // Domain
        let domain_offset = nt_response_offset + nt_response_len as u32;
        message.extend_from_slice(&domain_len.to_le_bytes());
        message.extend_from_slice(&domain_len.to_le_bytes());
        message.extend_from_slice(&domain_offset.to_le_bytes());

        // Username
        let username_offset = domain_offset + domain_len as u32;
        message.extend_from_slice(&username_len.to_le_bytes());
        message.extend_from_slice(&username_len.to_le_bytes());
        message.extend_from_slice(&username_offset.to_le_bytes());

        // Workstation
        let workstation_offset = username_offset + username_len as u32;
        message.extend_from_slice(&workstation_len.to_le_bytes());
        message.extend_from_slice(&workstation_len.to_le_bytes());
        message.extend_from_slice(&workstation_offset.to_le_bytes());

        // Session Key (empty)
        message.extend_from_slice(&0u16.to_le_bytes());
        message.extend_from_slice(&0u16.to_le_bytes());
        let session_key_offset = workstation_offset + workstation_len as u32;
        message.extend_from_slice(&session_key_offset.to_le_bytes());

        // Flags
        let flags = NTLM_FLAGS_NEGOTIATE_UNICODE
            | NTLM_FLAGS_NEGOTIATE_NTLM
            | NTLM_FLAGS_NEGOTIATE_ALWAYS_SIGN
            | NTLM_FLAGS_NEGOTIATE_EXTENDED_SESSIONSECURITY;
        message.extend_from_slice(&flags.to_le_bytes());

        // Version
        message.extend_from_slice(&NTLM_VERSION);

        // MIC (16 bytes, zero for now)
        message.extend_from_slice(&[0u8; 16]);

        // Payload
        // LM Response (zeros)
        message.extend_from_slice(&[0u8; 8]);

        // NT Response
        message.extend_from_slice(nt_response);

        // Domain
        if !domain.is_empty() {
            message.extend_from_slice(domain);
        }

        // Username
        message.extend_from_slice(username);

        // Workstation
        if !workstation.is_empty() {
            message.extend_from_slice(workstation);
        }

        BASE64.encode(&message)
    }

    /// 执行完整的 NTLM 认证流程
    ///
    /// 返回 Proxy-Authorization 头的值
    pub fn authenticate(&self, type2_message: &str) -> Result<String> {
        let type2 = self.parse_type2_message(type2_message)?;
        let type3 = self.create_type3_message(&type2);
        Ok(format!("NTLM {}", type3))
    }
}

/// Type 2 消息 (Challenge)
#[derive(Debug)]
pub struct Type2Message {
    /// 服务器挑战
    pub challenge: [u8; 8],
    /// 目标信息
    pub target_info: Vec<u8>,
    /// 标志
    pub flags: u32,
}

// NTLM 标志
const NTLM_FLAGS_NEGOTIATE_UNICODE: u32 = 0x00000001;
const NTLM_FLAGS_NEGOTIATE_OEM: u32 = 0x00000002;
const NTLM_FLAGS_REQUEST_TARGET: u32 = 0x00000004;
const NTLM_FLAGS_NEGOTIATE_NTLM: u32 = 0x00000200;
const NTLM_FLAGS_NEGOTIATE_ALWAYS_SIGN: u32 = 0x00008000;
const NTLM_FLAGS_NEGOTIATE_EXTENDED_SESSIONSECURITY: u32 = 0x00080000;
const NTLM_FLAGS_NEGOTIATE_VERSION: u32 = 0x02000000;
const NTLM_FLAGS_NEGOTIATE_128: u32 = 0x20000000;
const NTLM_FLAGS_NEGOTIATE_KEY_EXCH: u32 = 0x40000000;

// NTLM 版本 (Windows 10)
const NTLM_VERSION: [u8; 8] = [0x0a, 0x00, 0x63, 0x45, 0x00, 0x00, 0x00, 0x0f];

/// 计算 NTLM 哈希
fn compute_ntlm_hash(password: &str) -> [u8; 16] {
    // Convert password to UTF-16LE
    let password_utf16: Vec<u8> = password
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();

    // MD4 hash
    let mut hasher = Md4::new();
    hasher.update(&password_utf16);
    let result = hasher.finalize();

    let mut hash = [0u8; 16];
    hash.copy_from_slice(&result);
    hash
}

/// 计算 NTLMv1 响应
fn compute_ntlm_response_v1(ntlm_hash: &[u8; 16], challenge: &[u8; 8]) -> Vec<u8> {
    let mut response = Vec::with_capacity(24);

    // DES encrypt challenge with hash
    let key1 = create_des_key(&ntlm_hash[0..7]);
    let key2 = create_des_key(&ntlm_hash[7..14]);
    let key3 = create_des_key(&[
        ntlm_hash[14], ntlm_hash[15], 0, 0, 0, 0, 0,
    ]);

    let des1 = Des::new_from_slice(&key1).unwrap();
    let des2 = Des::new_from_slice(&key2).unwrap();
    let des3 = Des::new_from_slice(&key3).unwrap();

    let mut block1 = [0u8; 8];
    block1.copy_from_slice(challenge);
    des1.encrypt_block((&mut block1).into());
    response.extend_from_slice(&block1);

    let mut block2 = [0u8; 8];
    block2.copy_from_slice(challenge);
    des2.encrypt_block((&mut block2).into());
    response.extend_from_slice(&block2);

    let mut block3 = [0u8; 8];
    block3.copy_from_slice(challenge);
    des3.encrypt_block((&mut block3).into());
    response.extend_from_slice(&block3);

    response
}

/// 计算 NTLMv2 响应
fn compute_ntlm_response_v2(
    ntlm_hash: &[u8; 16],
    username: &str,
    domain: &str,
    server_challenge: &[u8; 8],
    target_info: &[u8],
) -> Vec<u8> {
    // Compute NTLMv2 hash
    let ntlmv2_hash = compute_ntlmv2_hash(ntlm_hash, username, domain);

    // Generate client challenge (blob)
    let mut client_challenge = [0u8; 8];
    thread_rng().fill_bytes(&mut client_challenge);
    let timestamp = get_ntlm_timestamp();

    // Build NTLMv2 blob
    let mut blob = Vec::new();
    blob.push(0x01); // Blob signature
    blob.push(0x01);
    blob.push(0x00);
    blob.push(0x00);

    blob.extend_from_slice(&[0u8; 4]); // Reserved

    blob.extend_from_slice(&timestamp); // Timestamp (64-bit)

    blob.extend_from_slice(&client_challenge); // Client challenge

    blob.extend_from_slice(&[0u8; 4]); // Reserved

    blob.extend_from_slice(target_info); // Target info

    blob.extend_from_slice(&[0u8; 4]); // Reserved

    // Compute NT proof string using HMAC-MD5
    let mut temp = Vec::new();
    temp.extend_from_slice(server_challenge);
    temp.extend_from_slice(&blob);

    let mut mac = <Hmac<Md5> as hmac::Mac>::new_from_slice(&ntlmv2_hash).unwrap();
    hmac::Mac::update(&mut mac, &temp);
    let nt_proof_str = mac.finalize().into_bytes();

    // Build NT response
    let mut nt_response = Vec::new();
    nt_response.extend_from_slice(&nt_proof_str);
    nt_response.extend_from_slice(&blob);

    nt_response
}

/// 计算 NTLMv2 哈希
fn compute_ntlmv2_hash(ntlm_hash: &[u8; 16], username: &str, domain: &str) -> [u8; 16] {
    // HMAC-MD5 of uppercase(username) + domain
    let mut data = Vec::new();
    data.extend_from_slice(username.to_uppercase().as_bytes());
    data.extend_from_slice(domain.to_uppercase().as_bytes());

    let mut mac = <Hmac<Md5> as hmac::Mac>::new_from_slice(ntlm_hash).unwrap();
    hmac::Mac::update(&mut mac, &data);
    let result = mac.finalize().into_bytes();

    let mut hash = [0u8; 16];
    hash.copy_from_slice(&result);
    hash
}

/// 获取 NTLM 时间戳
fn get_ntlm_timestamp() -> [u8; 8] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    // NTLM timestamp is 100-nanosecond intervals since Jan 1, 1601
    // Unix epoch is 134774 days after Jan 1, 1601
    let ntlm_epoch_offset: u64 = 134_774 * 24 * 60 * 60 * 10_000_000;
    let timestamp = now * 10 + ntlm_epoch_offset;

    timestamp.to_le_bytes()
}

/// 创建 DES 密钥 (添加奇偶校验位)
fn create_des_key(key: &[u8]) -> [u8; 8] {
    let mut des_key = [0u8; 8];
    for i in 0..7 {
        des_key[i] = key[i];
    }
    des_key[7] = 0;
    des_key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntlm_hash() {
        let hash = compute_ntlm_hash("password");
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_type1_message() {
        let config = NtlmConfig::new("user", "pass");
        let auth = NtlmAuthenticator::new(config);
        let msg = auth.create_type1_message();
        assert!(msg.starts_with("TlRMTVNT")); // NTLMSSP base64 prefix
    }

    #[test]
    fn test_ntlmv2_hash() {
        let ntlm_hash = compute_ntlm_hash("password");
        let v2_hash = compute_ntlmv2_hash(&ntlm_hash, "user", "domain");
        assert_eq!(v2_hash.len(), 16);
    }

    #[test]
    fn test_ntlm_config_builder() {
        let config = NtlmConfig::new("user", "pass")
            .domain("DOMAIN")
            .workstation("WORKSTATION")
            .version(NtlmVersion::V2);

        assert_eq!(config.username, "user");
        assert_eq!(config.password, "pass");
        assert_eq!(config.domain, "DOMAIN");
        assert_eq!(config.workstation, "WORKSTATION");
        assert_eq!(config.version, NtlmVersion::V2);
    }
}
