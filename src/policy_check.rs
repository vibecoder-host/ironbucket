use serde_json;
use tracing::debug;

// Check if an action is allowed based on bucket policy with IP conditions
pub fn check_policy_permission(
    policy_json: &str,
    action: &str,
    resource: &str,
    principal: &str,
    client_ip: Option<&str>,
) -> bool {
    debug!("Checking policy permission: action={}, resource={}, principal={}, client_ip={:?}",
           action, resource, principal, client_ip);

    // Parse the policy
    if let Ok(policy) = serde_json::from_str::<serde_json::Value>(policy_json) {
        if let Some(statements) = policy.get("Statement").and_then(|s| s.as_array()) {
            for statement in statements {
                // Check Effect
                let effect = statement.get("Effect")
                    .and_then(|e| e.as_str())
                    .unwrap_or("");

                // Check Principal
                let principal_match = if let Some(p) = statement.get("Principal") {
                    if p.as_str() == Some("*") || p == "*" {
                        true
                    } else if let Some(aws) = p.get("AWS") {
                        if let Some(arr) = aws.as_array() {
                            arr.iter().any(|v| v.as_str() == Some(principal))
                        } else {
                            aws.as_str() == Some(principal)
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Check Action
                let action_match = if let Some(actions) = statement.get("Action") {
                    if let Some(arr) = actions.as_array() {
                        arr.iter().any(|a| {
                            if let Some(act) = a.as_str() {
                                act == action || act == "s3:*" ||
                                (act.ends_with("*") && action.starts_with(&act[..act.len()-1]))
                            } else {
                                false
                            }
                        })
                    } else if let Some(act) = actions.as_str() {
                        act == action || act == "s3:*" ||
                        (act.ends_with("*") && action.starts_with(&act[..act.len()-1]))
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Check Resource
                let resource_match = if let Some(resources) = statement.get("Resource") {
                    if let Some(arr) = resources.as_array() {
                        arr.iter().any(|r| {
                            if let Some(res) = r.as_str() {
                                res == resource || res == "*" ||
                                (res.ends_with("*") && resource.starts_with(&res[..res.len()-1]))
                            } else {
                                false
                            }
                        })
                    } else if let Some(res) = resources.as_str() {
                        res == resource || res == "*" ||
                        (res.ends_with("*") && resource.starts_with(&res[..res.len()-1]))
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Check Conditions (including IP address)
                let condition_match = if let Some(conditions) = statement.get("Condition") {
                    let mut all_conditions_met = true;

                    // Check IpAddress condition
                    if let Some(ip_condition) = conditions.get("IpAddress") {
                        if let Some(source_ip_condition) = ip_condition.get("aws:SourceIp") {
                            if let Some(client_ip_str) = client_ip {
                                let ip_allowed = if let Some(arr) = source_ip_condition.as_array() {
                                    arr.iter().any(|allowed_ip| {
                                        if let Some(ip_str) = allowed_ip.as_str() {
                                            is_ip_in_range(client_ip_str, ip_str)
                                        } else {
                                            false
                                        }
                                    })
                                } else if let Some(ip_str) = source_ip_condition.as_str() {
                                    is_ip_in_range(client_ip_str, ip_str)
                                } else {
                                    false
                                };
                                if !ip_allowed {
                                    debug!("IP condition not met: client_ip={} not in allowed range", client_ip_str);
                                    all_conditions_met = false;
                                }
                            } else {
                                // No client IP available, condition fails
                                debug!("IP condition not met: no client IP available");
                                all_conditions_met = false;
                            }
                        }
                    }

                    // Check NotIpAddress condition
                    if let Some(not_ip_condition) = conditions.get("NotIpAddress") {
                        if let Some(source_ip_condition) = not_ip_condition.get("aws:SourceIp") {
                            if let Some(client_ip_str) = client_ip {
                                let ip_blocked = if let Some(arr) = source_ip_condition.as_array() {
                                    arr.iter().any(|blocked_ip| {
                                        if let Some(ip_str) = blocked_ip.as_str() {
                                            is_ip_in_range(client_ip_str, ip_str)
                                        } else {
                                            false
                                        }
                                    })
                                } else if let Some(ip_str) = source_ip_condition.as_str() {
                                    is_ip_in_range(client_ip_str, ip_str)
                                } else {
                                    false
                                };
                                if ip_blocked {
                                    debug!("NotIpAddress condition not met: client_ip={} is in blocked range", client_ip_str);
                                    all_conditions_met = false;
                                }
                            }
                        }
                    }

                    all_conditions_met
                } else {
                    // No conditions, always match
                    true
                };

                // If all conditions match (including IP conditions)
                if principal_match && action_match && resource_match && condition_match {
                    debug!("Statement matched with effect: {}", effect);
                    if effect == "Allow" {
                        return true;
                    } else if effect == "Deny" {
                        return false;
                    }
                }
            }
        }
    }

    // Default deny if no matching statement
    debug!("No matching statement found, denying access");
    false
}

// Helper function to check if an IP is in a CIDR range
pub fn is_ip_in_range(ip: &str, range: &str) -> bool {
    use std::net::{IpAddr, Ipv4Addr};

    // Parse the IP address
    let client_ip = match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(addr)) => addr,
        _ => {
            debug!("Failed to parse client IP: {}", ip);
            return false;
        }
    };

    // Check if range is a CIDR notation
    if let Some(slash_pos) = range.find('/') {
        let (network_str, prefix_str) = range.split_at(slash_pos);
        let prefix_len: u8 = match prefix_str[1..].parse() {
            Ok(len) if len <= 32 => len,
            _ => {
                debug!("Invalid CIDR prefix length: {}", prefix_str);
                return false;
            }
        };

        let network_ip = match network_str.parse::<Ipv4Addr>() {
            Ok(addr) => addr,
            _ => {
                debug!("Failed to parse network IP: {}", network_str);
                return false;
            }
        };

        // Create mask
        let mask = if prefix_len == 0 {
            0
        } else {
            !((1u32 << (32 - prefix_len)) - 1)
        };

        // Convert IPs to u32 for comparison
        let client_u32 = u32::from_be_bytes(client_ip.octets());
        let network_u32 = u32::from_be_bytes(network_ip.octets());

        // Check if client IP is in the network range
        let in_range = (client_u32 & mask) == (network_u32 & mask);
        debug!("IP range check: {} in {} = {}", ip, range, in_range);
        in_range
    } else {
        // Single IP address comparison
        match range.parse::<Ipv4Addr>() {
            Ok(allowed_ip) => {
                let matches = client_ip == allowed_ip;
                debug!("IP exact match check: {} == {} = {}", ip, range, matches);
                matches
            }
            _ => {
                debug!("Failed to parse allowed IP: {}", range);
                false
            }
        }
    }
}