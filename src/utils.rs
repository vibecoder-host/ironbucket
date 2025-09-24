use chrono::{DateTime, Utc};
use hmac::Hmac;
use sha2::Sha256;

pub type HmacSha256 = Hmac<Sha256>;

// Helper function to format date for HTTP Last-Modified header (RFC2822 with GMT)
pub fn format_http_date(dt: &DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

// Helper function to parse AWS chunked transfer encoding with signatures
pub fn parse_chunked_data(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Find the chunk size (before semicolon)
        let chunk_header_end = match find_sequence(&input[pos..], b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };

        let header = &input[pos..chunk_header_end];
        let header_str = String::from_utf8_lossy(header);

        // Parse chunk size (hex before semicolon or end of header)
        let size_str = if let Some(semi_pos) = header_str.find(';') {
            &header_str[..semi_pos]
        } else {
            &header_str
        };

        // Parse hex chunk size
        let chunk_size = match usize::from_str_radix(size_str.trim(), 16) {
            Ok(size) => size,
            Err(_) => break,
        };

        // Skip past header and \r\n
        pos = chunk_header_end + 2;

        // If chunk size is 0, we're done
        if chunk_size == 0 {
            break;
        }

        // Read chunk data
        if pos + chunk_size <= input.len() {
            result.extend_from_slice(&input[pos..pos + chunk_size]);
            pos += chunk_size;

            // Skip trailing \r\n after chunk
            if pos + 2 <= input.len() && &input[pos..pos + 2] == b"\r\n" {
                pos += 2;
            }
        } else {
            break;
        }
    }

    // If no chunks were parsed, return original data
    if result.is_empty() {
        input.to_vec()
    } else {
        result
    }
}

// Helper to find a byte sequence in a slice
pub fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len())
        .position(|window| window == needle)
}