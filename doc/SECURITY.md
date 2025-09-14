# Security Guide

Comprehensive security documentation for IronBucket, covering authentication, encryption, access control, and best practices.

## Table of Contents

- [Authentication](#authentication)
- [Authorization](#authorization)
- [Encryption](#encryption)
- [Access Control](#access-control)
- [Network Security](#network-security)
- [Audit & Compliance](#audit--compliance)
- [Security Best Practices](#security-best-practices)
- [Vulnerability Management](#vulnerability-management)
- [Incident Response](#incident-response)
- [Security Checklist](#security-checklist)

## Authentication

### AWS Signature Version 4

IronBucket implements AWS Signature Version 4 for authentication, ensuring secure API access.

#### How It Works

1. Client creates a canonical request
2. Client creates a string to sign
3. Client calculates signature using secret key
4. Client adds signature to request
5. Server validates signature

#### Configuration

```bash
# Basic authentication setup
export ACCESS_KEY=your-secure-access-key
export SECRET_KEY=your-very-secure-secret-key
```


### Bucket Policies

#### Policy Examples

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicReadGetObject",
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::public-bucket/*",
      "Condition": {
        "IpAddress": {
          "aws:SourceIp": ["192.168.1.0/24"]
        }
      }
    },
    {
      "Sid": "DenyUnencryptedObjectUploads",
      "Effect": "Deny",
      "Principal": "*",
      "Action": "s3:PutObject",
      "Resource": "arn:aws:s3:::secure-bucket/*",
      "Condition": {
        "StringNotEquals": {
          "s3:x-amz-server-side-encryption": "AES256"
        }
      }
    },
    {
      "Sid": "RestrictDeleteToAdmin",
      "Effect": "Deny",
      "NotPrincipal": {
        "AWS": "arn:aws:iam::ACCOUNT:user/admin"
      },
      "Action": "s3:DeleteObject",
      "Resource": "arn:aws:s3:::critical-data/*"
    }
  ]
}
```

#### Applying Policies

```bash
# Apply bucket policy
aws s3api put-bucket-policy \
  --endpoint-url $IRONBUCKET_ENDPOINT \
  --bucket my-bucket \
  --policy file://bucket-policy.json

# Get current policy
aws s3api get-bucket-policy \
  --endpoint-url $IRONBUCKET_ENDPOINT \
  --bucket my-bucket
```

## Encryption

### Server-Side Encryption

#### Configuration

```bash
# Enable server-side encryption
export ENABLE_ENCRYPTION=true
export ENCRYPTION_ALGORITHM=AES256-GCM

# Master key (base64 encoded 256-bit key)
export ENCRYPTION_MASTER_KEY=$(openssl rand -base64 32)

# Key rotation period
export KEY_ROTATION_DAYS=90
```


## Access Control

### IP Whitelisting

```nginx
# nginx configuration for IP restrictions
server {
    listen 443 ssl;
    server_name s3.example.com;

    # Allow internal network
    allow 192.168.1.0/24;
    allow 10.0.0.0/8;

    # Allow specific IPs
    allow 203.0.113.5;
    allow 203.0.113.10;

    # Deny all others
    deny all;

    location / {
        proxy_pass http://localhost:9000;
    }
}
```



#### nginx Rate Limiting

```nginx
http {
    # Define rate limit zones
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
    limit_req_zone $binary_remote_addr zone=uploads:10m rate=5r/s;
    limit_req_zone $binary_remote_addr zone=downloads:10m rate=50r/s;

    server {
        # API endpoints
        location /api/ {
            limit_req zone=api burst=20 nodelay;
            proxy_pass http://localhost:9000;
        }

        # Upload endpoints
        location ~ ^/[^/]+/[^/]+$ {
            if ($request_method = PUT) {
                limit_req zone=uploads burst=10;
            }
            proxy_pass http://localhost:9000;
        }

        # Download endpoints
        location ~ ^/[^/]+/[^/]+$ {
            if ($request_method = GET) {
                limit_req zone=downloads burst=100 nodelay;
            }
            proxy_pass http://localhost:9000;
        }
    }
}
```

### CORS Configuration

```toml
# config.toml
[cors]
enabled = true
allowed_origins = [
    "https://app.example.com",
    "https://admin.example.com"
]
allowed_methods = ["GET", "PUT", "POST", "DELETE", "HEAD", "OPTIONS"]
allowed_headers = [
    "Authorization",
    "Content-Type",
    "X-Amz-Date",
    "X-Amz-Security-Token"
]
exposed_headers = ["ETag", "X-Amz-Version-Id"]
max_age = 86400
allow_credentials = true
```

## Network Security

### TLS/SSL Configuration

```bash
# Generate certificates
openssl req -x509 -nodes -days 365 -newkey rsa:4096 \
  -keyout /etc/ironbucket/key.pem \
  -out /etc/ironbucket/cert.pem

# Enable TLS
export ENABLE_TLS=true
export TLS_CERT=/etc/ironbucket/cert.pem
export TLS_KEY=/etc/ironbucket/key.pem
export TLS_MIN_VERSION=TLS1.3
```

#### nginx SSL Configuration

```nginx
server {
    listen 443 ssl http2;
    server_name s3.example.com;

    # SSL certificates
    ssl_certificate /etc/letsencrypt/live/s3.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/s3.example.com/privkey.pem;

    # SSL configuration
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;

    # OCSP stapling
    ssl_stapling on;
    ssl_stapling_verify on;
    ssl_trusted_certificate /etc/letsencrypt/live/s3.example.com/chain.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;

    location / {
        proxy_pass http://localhost:9000;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Firewall Configuration

```bash
# iptables rules
iptables -A INPUT -i lo -j ACCEPT
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A INPUT -p tcp --dport 22 -s 192.168.1.0/24 -j ACCEPT
iptables -A INPUT -p tcp --dport 443 -j ACCEPT
iptables -A INPUT -p tcp --dport 9000 -s 127.0.0.1 -j ACCEPT
iptables -A INPUT -j DROP

# Save rules
iptables-save > /etc/iptables/rules.v4
```


### Operational Security

1. **Regular updates**
   ```bash
   # Update IronBucket
   cargo update
   cargo build --release

   # Update dependencies
   cargo audit
   ```

2. **Security scanning**
   ```bash
   # Scan for vulnerabilities
   trivy fs /opt/ironbucket

   # Static analysis
   cargo clippy -- -D warnings
   ```

3. **Penetration testing**
   ```bash
   # Use OWASP ZAP
   zap-cli quick-scan --self-contained \
     --start-options '-config api.disablekey=true' \
     http://localhost:9000
   ```

## Security Checklist

### Pre-Deployment

- [ ] Strong access keys generated
- [ ] Secret keys properly hashed
- [ ] TLS/SSL certificates valid
- [ ] Firewall rules configured
- [ ] SELinux/AppArmor enabled
- [ ] Audit logging enabled
- [ ] Encryption enabled
- [ ] Rate limiting configured
- [ ] CORS properly configured
- [ ] IP whitelisting implemented

### Operational

- [ ] Regular key rotation schedule
- [ ] Audit logs monitored
- [ ] Security updates applied
- [ ] Vulnerability scans performed
- [ ] Penetration tests conducted
- [ ] Incident response plan tested
- [ ] Backup and recovery tested
- [ ] Access reviews conducted
- [ ] Compliance checks passed
- [ ] Security training completed

### Monitoring

- [ ] Failed authentication attempts tracked
- [ ] Large data transfers monitored
- [ ] Deletion events logged
- [ ] Suspicious IPs blocked
- [ ] Certificate expiration monitored
- [ ] Disk encryption verified
- [ ] Resource usage monitored
- [ ] API rate limits enforced
- [ ] Security alerts configured
- [ ] Log retention policy enforced

## Security Tools

### Recommended Tools

1. **Scanning**: Trivy, Grype, OWASP ZAP
2. **Monitoring**: Prometheus, Grafana, ELK Stack
3. **Secrets**: HashiCorp Vault, AWS Secrets Manager
4. **WAF**: ModSecurity, Cloudflare
5. **SIEM**: Splunk, Elastic Security
6. **Compliance**: Open Policy Agent, Falco

## Next Steps

- [Configuration Guide](./CONFIGURATION.md) - Secure configuration options
- [Performance Guide](./PERFORMANCE.md) - Performance with security
- [Troubleshooting Guide](./TROUBLESHOOTING.md) - Security issue resolution