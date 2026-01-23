// src/Builtins/Static/ip_address_object.rs
//! IpAddress static object for IP address validation and manipulation
//! Uses std::net::IpAddr - zero external dependencies

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// IpAddress static object implementation
pub struct IpAddressObject {
    base: StaticObjectBase,
}

impl IpAddressObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("IpAddress".to_string());
        Self::initialize_methods(&mut base);
        IpAddressObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // IpAddress.parse(str) - Parse IP address from string
        base.register_method(Box::new(BuiltinMethod::new(
            "parse".to_string(),
            1,
            DixType::String,
            |args| {
                let input = args[0].as_string();

                input.parse::<IpAddr>()
                    .map(|ip| DixValue::from_string(ip.to_string()))
                    .map_err(|_| format!("Invalid IP address format: {}", input))
            },
            "Parses an IP address from string (throws error if invalid)".to_string(),
        )));

        // IpAddress.tryParse(str) - Try parse, return null on failure
        base.register_method(Box::new(BuiltinMethod::new(
            "tryParse".to_string(),
            1,
            DixType::String,
            |args| {
                let input = args[0].as_string();

                Ok(match input.parse::<IpAddr>() {
                    Ok(ip) => DixValue::from_string(ip.to_string()),
                    Err(_) => DixValue::null(),
                })
            },
            "Tries to parse an IP address, returns null if invalid".to_string(),
        )));

        // IpAddress.validate(str) - Check if valid IP address
        base.register_method(Box::new(BuiltinMethod::new(
            "validate".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();
                Ok(DixValue::from_bool(input.parse::<IpAddr>().is_ok()))
            },
            "Checks if a string is a valid IP address".to_string(),
        )));

        // IpAddress.isV4(str) - Check if IPv4
        base.register_method(Box::new(BuiltinMethod::new(
            "isV4".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();

                let is_v4 = input.parse::<IpAddr>()
                    .map(|ip| ip.is_ipv4())
                    .unwrap_or(false);

                Ok(DixValue::from_bool(is_v4))
            },
            "Checks if an IP address is IPv4".to_string(),
        )));

        // IpAddress.isV6(str) - Check if IPv6
        base.register_method(Box::new(BuiltinMethod::new(
            "isV6".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();

                let is_v6 = input.parse::<IpAddr>()
                    .map(|ip| ip.is_ipv6())
                    .unwrap_or(false);

                Ok(DixValue::from_bool(is_v6))
            },
            "Checks if an IP address is IPv6".to_string(),
        )));

        // IpAddress.isPrivate(str) - Check if private range
        base.register_method(Box::new(BuiltinMethod::new(
            "isPrivate".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();

                let ip = input.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid IP address: {}", input))?;

                Ok(DixValue::from_bool(is_private_ip(&ip)))
            },
            "Checks if an IP address is in a private range (10.x, 172.16-31.x, 192.168.x, fc00::/7)".to_string(),
        )));

        // IpAddress.isLoopback(str) - Check if loopback
        base.register_method(Box::new(BuiltinMethod::new(
            "isLoopback".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();

                let ip = input.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid IP address: {}", input))?;

                Ok(DixValue::from_bool(ip.is_loopback()))
            },
            "Checks if an IP address is a loopback address (127.0.0.1, ::1)".to_string(),
        )));

        // IpAddress.isPublic(str) - Check if public IP
        base.register_method(Box::new(BuiltinMethod::new(
            "isPublic".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();

                let ip = input.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid IP address: {}", input))?;

                let is_public = !is_private_ip(&ip)
                    && !ip.is_loopback()
                    && !is_link_local(&ip);

                Ok(DixValue::from_bool(is_public))
            },
            "Checks if an IP address is a public (routable) address".to_string(),
        )));

        // IpAddress.toBytes(str) - Convert IP to byte array
        base.register_method(Box::new(BuiltinMethod::new(
            "toBytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let input = args[0].as_string();

                let ip = input.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid IP address: {}", input))?;

                let bytes: Vec<DixValue> = match ip {
                    IpAddr::V4(ipv4) => ipv4.octets().iter()
                        .map(|&b| DixValue::from_int(b as i32))
                        .collect(),
                    IpAddr::V6(ipv6) => ipv6.octets().iter()
                        .map(|&b| DixValue::from_int(b as i32))
                        .collect(),
                };

                Ok(DixValue::from_array(bytes))
            },
            "Converts an IP address to a byte array (4 bytes for IPv4, 16 for IPv6)".to_string(),
        )));

        // IpAddress.fromBytes(array) - Create IP from byte array
        base.register_method(Box::new(BuiltinMethod::new(
            "fromBytes".to_string(),
            1,
            DixType::String,
            |args| {
                let byte_array = args[0].as_array();

                if byte_array.len() != 4 && byte_array.len() != 16 {
                    return Err(format!(
                        "IP address requires 4 bytes (IPv4) or 16 bytes (IPv6), got {}",
                        byte_array.len()
                    ));
                }

                let ip = if byte_array.len() == 4 {
                    let mut octets = [0u8; 4];
                    for (i, val) in byte_array.iter().enumerate() {
                        let byte_val = val.as_int();
                        if !(0..=255).contains(&byte_val) {
                            return Err(format!("Byte value must be 0-255, got {}", byte_val));
                        }
                        octets[i] = byte_val as u8;
                    }
                    IpAddr::V4(Ipv4Addr::from(octets))
                } else {
                    let mut octets = [0u8; 16];
                    for (i, val) in byte_array.iter().enumerate() {
                        let byte_val = val.as_int();
                        if !(0..=255).contains(&byte_val) {
                            return Err(format!("Byte value must be 0-255, got {}", byte_val));
                        }
                        octets[i] = byte_val as u8;
                    }
                    IpAddr::V6(Ipv6Addr::from(octets))
                };

                Ok(DixValue::from_string(ip.to_string()))
            },
            "Creates an IP address from a byte array (4 bytes for IPv4, 16 for IPv6)".to_string(),
        )));

        // IpAddress.inRange(ip, rangeStart, rangeEnd) - Check if IP in range
        base.register_method(Box::new(BuiltinMethod::new(
            "inRange".to_string(),
            3,
            DixType::Bool,
            |args| {
                let ip_str = args[0].as_string();
                let start_str = args[1].as_string();
                let end_str = args[2].as_string();

                let ip = ip_str.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid IP address: {}", ip_str))?;
                let start = start_str.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid start IP: {}", start_str))?;
                let end = end_str.parse::<IpAddr>()
                    .map_err(|_| format!("Invalid end IP: {}", end_str))?;

                // All must be same version
                if (ip.is_ipv4() != start.is_ipv4()) || (ip.is_ipv4() != end.is_ipv4()) {
                    return Err("All IP addresses must be same version (IPv4 or IPv6)".to_string());
                }

                let ip_bytes = match ip {
                    IpAddr::V4(ipv4) => ipv4.octets().to_vec(),
                    IpAddr::V6(ipv6) => ipv6.octets().to_vec(),
                };
                let start_bytes = match start {
                    IpAddr::V4(ipv4) => ipv4.octets().to_vec(),
                    IpAddr::V6(ipv6) => ipv6.octets().to_vec(),
                };
                let end_bytes = match end {
                    IpAddr::V4(ipv4) => ipv4.octets().to_vec(),
                    IpAddr::V6(ipv6) => ipv6.octets().to_vec(),
                };

                let in_range = compare_ip_bytes(&ip_bytes, &start_bytes) >= 0
                    && compare_ip_bytes(&ip_bytes, &end_bytes) <= 0;

                Ok(DixValue::from_bool(in_range))
            },
            "Checks if an IP address is within a specified range (inclusive)".to_string(),
        )));

        // IpAddress.localhost() - Get localhost IP
        base.register_method(Box::new(BuiltinMethod::new(
            "localhost".to_string(),
            0,
            DixType::String,
            |_args| {
                Ok(DixValue::from_string("127.0.0.1".to_string()))
            },
            "Returns the IPv4 localhost address (127.0.0.1)".to_string(),
        )));

        // IpAddress.any() - Get "any" IP (0.0.0.0)
        base.register_method(Box::new(BuiltinMethod::new(
            "any".to_string(),
            0,
            DixType::String,
            |_args| {
                Ok(DixValue::from_string("0.0.0.0".to_string()))
            },
            "Returns the IPv4 'any' address (0.0.0.0)".to_string(),
        )));

        // IpAddress.broadcast() - Get broadcast IP
        base.register_method(Box::new(BuiltinMethod::new(
            "broadcast".to_string(),
            0,
            DixType::String,
            |_args| {
                Ok(DixValue::from_string("255.255.255.255".to_string()))
            },
            "Returns the IPv4 broadcast address (255.255.255.255)".to_string(),
        )));
    }
}

impl Default for IpAddressObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for IpAddressObject {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool {
        self.base.has_method(method_name)
    }

    fn get_method_names(&self) -> Vec<String> {
        self.base.get_method_names()
    }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

// ==================== HELPER FUNCTIONS ====================

/// Check if IP is in private range
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 10.0.0.0/8
            octets[0] == 10 ||
                // 172.16.0.0/12
                (octets[0] == 172 && (16..=31).contains(&octets[1])) ||
                // 192.168.0.0/16
                (octets[0] == 192 && octets[1] == 168)
        }
        IpAddr::V6(ipv6) => {
            let octets = ipv6.octets();
            // fc00::/7 (unique local addresses)
            (octets[0] & 0xFE) == 0xFC
        }
    }
}

/// Check if IP is link-local
fn is_link_local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 169.254.0.0/16
            octets[0] == 169 && octets[1] == 254
        }
        IpAddr::V6(ipv6) => {
            let octets = ipv6.octets();
            // fe80::/10
            (octets[0] & 0xFF) == 0xFE && (octets[1] & 0xC0) == 0x80
        }
    }
}

/// Compare two IP address byte arrays
fn compare_ip_bytes(a: &[u8], b: &[u8]) -> i32 {
    for (byte_a, byte_b) in a.iter().zip(b.iter()) {
        if byte_a < byte_b {
            return -1;
        }
        if byte_a > byte_b {
            return 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_address_object_creation() {
        let ip_obj = IpAddressObject::new();
        assert_eq!(ip_obj.name(), "IpAddress");
    }

    #[test]
    fn test_validate_ipv4() {
        let ip_obj = IpAddressObject::new();
        let result = ip_obj.call_method(
            "validate",
            &[DixValue::from_string("192.168.1.1".to_string())],
        ).unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_is_private() {
        let ip_obj = IpAddressObject::new();
        let result = ip_obj.call_method(
            "isPrivate",
            &[DixValue::from_string("192.168.1.1".to_string())],
        ).unwrap();
        assert!(result.as_bool());
    }
}