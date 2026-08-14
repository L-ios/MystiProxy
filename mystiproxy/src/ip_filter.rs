//! 入站 IP 过滤模块
//!
//! 提供基于 CIDR 的允许/拒绝列表，作用于 TCP 与 HTTP 引擎的 accept 层。
//! 语义：deny 优先；allow 非空时仅放行命中项；两者均未配置则不过滤。

use std::net::IpAddr;

use crate::error::{MystiProxyError, Result};

/// 单条 CIDR 规则
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cidr {
    net: IpAddr,
    prefix: u32,
}

impl Cidr {
    fn parse(s: &str) -> Result<Self> {
        let (addr_part, prefix_part) = match s.rsplit_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };

        let addr: IpAddr = addr_part
            .parse()
            .map_err(|_| MystiProxyError::Config(format!("invalid IP in CIDR rule '{s}'")))?;

        let prefix = match (prefix_part, addr) {
            (Some(p), _) => p
                .parse::<u32>()
                .map_err(|_| MystiProxyError::Config(format!("invalid prefix in '{s}'")))?,
            (None, IpAddr::V4(_)) => 32,
            (None, IpAddr::V6(_)) => 128,
        };

        let max = if addr.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(MystiProxyError::Config(format!(
                "prefix /{prefix} too long for address family in '{s}'"
            )));
        }

        Ok(Self { net: addr, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        // v4 与 v6 互不匹配（不做隐式映射）
        match (self.net, ip) {
            (IpAddr::V4(_), IpAddr::V6(_)) | (IpAddr::V6(_), IpAddr::V4(_)) => return false,
            _ => {}
        }
        let (net_bits, ip_bits, bits) = match (self.net, ip) {
            (IpAddr::V4(n), IpAddr::V4(i)) => (u32::from(n) as u128, u32::from(i) as u128, 32),
            (IpAddr::V6(n), IpAddr::V6(i)) => (u128::from(n), u128::from(i), 128),
            _ => unreachable!(),
        };
        let shift = bits - self.prefix;
        (net_bits >> shift) == (ip_bits >> shift)
    }
}

/// 入站 IP 过滤器
#[derive(Debug, Clone, Default)]
pub struct IpFilter {
    allow: Vec<Cidr>,
    deny: Vec<Cidr>,
}

impl IpFilter {
    /// 从引擎配置构建；两者均 None/空时返回 None（不过滤）。
    /// 任何非法条目返回配置错误（fail-fast）。
    pub fn from_config(
        allow: &Option<Vec<String>>,
        deny: &Option<Vec<String>>,
    ) -> Result<Option<Self>> {
        let allow_empty = allow.as_ref().is_none_or(|v| v.is_empty());
        let deny_empty = deny.as_ref().is_none_or(|v| v.is_empty());
        if allow_empty && deny_empty {
            return Ok(None);
        }

        let parse_list = |v: &Option<Vec<String>>| -> Result<Vec<Cidr>> {
            v.as_ref()
                .map(|list| list.iter().map(|s| Cidr::parse(s)).collect())
                .transpose()
                .map(|opt| opt.unwrap_or_default())
        };

        Ok(Some(Self {
            allow: parse_list(allow)?,
            deny: parse_list(deny)?,
        }))
    }

    /// 判定 peer 是否放行
    pub fn is_allowed(&self, peer: IpAddr) -> bool {
        if self.deny.iter().any(|c| c.contains(peer)) {
            return false;
        }
        if self.allow.is_empty() {
            return true;
        }
        self.allow.iter().any(|c| c.contains(peer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    // ===== CIDR 解析 =====

    #[test]
    fn test_parse_v4_cidr() {
        let c = Cidr::parse("10.0.0.0/8").unwrap();
        assert_eq!(c.net, v4("10.0.0.0"));
        assert_eq!(c.prefix, 8);
    }

    #[test]
    fn test_parse_v6_cidr() {
        let c = Cidr::parse("2001:db8::/32").unwrap();
        assert_eq!(c.net, v6("2001:db8::"));
        assert_eq!(c.prefix, 32);
    }

    #[test]
    fn test_parse_bare_ip_defaults_full_prefix() {
        assert_eq!(Cidr::parse("1.2.3.4").unwrap().prefix, 32);
        assert_eq!(Cidr::parse("::1").unwrap().prefix, 128);
    }

    #[test]
    fn test_parse_invalid_ip() {
        assert!(Cidr::parse("999.1.1.1/8").is_err());
        assert!(Cidr::parse("nonsense/8").is_err());
    }

    #[test]
    fn test_parse_invalid_prefix() {
        assert!(Cidr::parse("10.0.0.0/abc").is_err());
        assert!(Cidr::parse("10.0.0.0/33").is_err());
        assert!(Cidr::parse("::1/129").is_err());
    }

    // ===== 匹配 =====

    #[test]
    fn test_match_v4_inside_outside() {
        let c = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(c.contains(v4("10.255.1.2")));
        assert!(!c.contains(v4("11.0.0.1")));
    }

    #[test]
    fn test_match_exact_32() {
        let c = Cidr::parse("192.168.1.5").unwrap();
        assert!(c.contains(v4("192.168.1.5")));
        assert!(!c.contains(v4("192.168.1.6")));
    }

    #[test]
    fn test_match_v6_loopback() {
        let c = Cidr::parse("::1/128").unwrap();
        assert!(c.contains(v6("::1")));
        assert!(!c.contains(v6("::2")));
    }

    #[test]
    fn test_match_v6_prefix() {
        let c = Cidr::parse("fd00::/8").unwrap();
        assert!(c.contains(v6("fd12:3456::9")));
        assert!(!c.contains(v6("fe80::1")));
    }

    #[test]
    fn test_cross_family_never_matches() {
        let c4 = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(!c4.contains(v6("::1")));
        let c6 = Cidr::parse("::/0").unwrap();
        assert!(!c6.contains(v4("1.2.3.4")));
    }

    #[test]
    fn test_zero_prefix_matches_all_same_family() {
        let c = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(c.contains(v4("203.0.113.7")));
    }

    // ===== 语义 =====

    #[test]
    fn test_from_config_none_when_empty() {
        assert!(IpFilter::from_config(&None, &None).unwrap().is_none());
        assert!(IpFilter::from_config(&Some(vec![]), &Some(vec![]))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_from_config_invalid_entry_fails() {
        let e = IpFilter::from_config(&Some(vec!["10.0.0.0/99".into()]), &None).unwrap_err();
        assert!(matches!(e, MystiProxyError::Config(_)));
    }

    #[test]
    fn test_allow_only_semantics() {
        let f = IpFilter::from_config(&Some(vec!["10.0.0.0/8".into()]), &None)
            .unwrap()
            .unwrap();
        assert!(f.is_allowed(v4("10.1.2.3")));
        assert!(!f.is_allowed(v4("127.0.0.1")));
    }

    #[test]
    fn test_deny_only_semantics() {
        let f = IpFilter::from_config(&None, &Some(vec!["192.168.1.5/32".into()]))
            .unwrap()
            .unwrap();
        assert!(!f.is_allowed(v4("192.168.1.5")));
        assert!(f.is_allowed(v4("192.168.1.6")));
        assert!(f.is_allowed(v4("8.8.8.8")));
    }

    #[test]
    fn test_deny_takes_priority_over_allow() {
        let f = IpFilter::from_config(
            &Some(vec!["10.0.0.0/8".into()]),
            &Some(vec!["10.66.0.0/16".into()]),
        )
        .unwrap()
        .unwrap();
        // 命中 allow 但也命中 deny → 拒绝
        assert!(!f.is_allowed(v4("10.66.1.1")));
        // 命中 allow 且不命中 deny → 放行
        assert!(f.is_allowed(v4("10.67.1.1")));
    }

    #[test]
    fn test_multiple_rules_any_match() {
        let f = IpFilter::from_config(
            &Some(vec!["10.0.0.0/8".into(), "192.168.0.0/16".into()]),
            &None,
        )
        .unwrap()
        .unwrap();
        assert!(f.is_allowed(v4("192.168.5.5")));
        assert!(f.is_allowed(v4("10.0.0.1")));
        assert!(!f.is_allowed(v4("172.16.0.1")));
    }
}
