# Security Policy

## Overview

This document outlines the security measures implemented in the YouTube Downloader application.

## Security Fixes Applied

### 1. Path Traversal Prevention (CWE-22)

**Issue**: Improper validation of file paths could allow attackers to access files outside the downloads directory.

**Fix**:

- Enhanced `validate_safe_path()` function used throughout the application
- All file operations now validate paths against the base downloads directory
- Prevents `../` and other directory traversal patterns

**Files affected**: `web_app.py`

### 2. Server-Side Request Forgery (SSRF) Prevention (CWE-918)

**Issue**: User-supplied URLs could be used to make requests to internal resources.

**Fix**:

- Added `validate_url()` function to check all incoming URLs
- Blocks localhost, private IP ranges (RFC 1918), loopback addresses
- Only allows http:// and https:// schemes
- Blocks internal domain patterns (.local, .internal, .corp, .lan)

**Endpoints protected**:

- `/api/video_info`
- `/api/download`
- `/api/debug_formats`

### 3. Command Injection Prevention (CWE-78)

**Issue**: File paths passed to system commands could be manipulated.

**Fix**:

- Explicitly set `shell=False` in all `subprocess.Popen()` calls
- Use list of arguments instead of string commands
- Validate all file paths before passing to system commands

**Files affected**: `web_app.py` (`open_file` endpoint)

### 4. Insecure Secret Key Generation (CWE-330)

**Issue**: Flask SECRET_KEY was generated on each startup using `os.urandom(24)`, making sessions invalid after restart.

**Fix**:

- Implemented `get_secret_key()` function with proper key management
- Priority order:
  1. `FLASK_SECRET_KEY` environment variable
  2. Persistent `.secret_key` file (auto-generated, 32 bytes)
- Key file permissions set to 0600 (owner read/write only)
- `.secret_key` added to `.gitignore`

### 5. Cross-Site Scripting (XSS) Prevention

**Issue**: Missing security headers could allow XSS attacks.

**Fix**:

- Added Content Security Policy (CSP) headers
- Policy includes:
  - `default-src 'self'` - Only load resources from same origin
  - `script-src 'self' 'unsafe-inline'` - Allow inline scripts (required for current implementation)
  - `frame-ancestors 'none'` - Prevent clickjacking
  - `base-uri 'self'` - Prevent base tag injection
- Added proper cache control headers
- Added `no-store` for dynamic content

### 6. Additional Security Headers

- `X-Content-Type-Options: nosniff` - Prevent MIME type sniffing
- `X-Frame-Options: DENY` - Prevent clickjacking
- `Referrer-Policy: no-referrer` - Don't send referrer information
- Removed deprecated `X-XSS-Protection` header

## Environment Variables

### Security Configuration

```bash
# Flask secret key (recommended for production)
export FLASK_SECRET_KEY="your-secure-random-key-here"

# API token for completed downloads endpoint (optional)
export DOWNLOADS_API_TOKEN="your-api-token-here"
```

### Generating Secure Keys

```bash
# Generate a secure secret key (Linux/macOS)
python3 -c "import os; print(os.urandom(32).hex())"

# Generate a secure secret key (Windows PowerShell)
python -c "import os; print(os.urandom(32).hex())"
```

## File Permissions

The application automatically sets secure permissions:

- `.secret_key` - 0600 (owner read/write only)

## Best Practices

### Deployment

1. **Always set FLASK_SECRET_KEY** in production:

   ```bash
   export FLASK_SECRET_KEY="$(python3 -c 'import os; print(os.urandom(32).hex())')"
   ```

2. **Use HTTPS** in production (enable Strict-Transport-Security):

   - Uncomment HSTS header in `add_security_headers()` function
   - Use a reverse proxy (nginx, Apache) with SSL certificate

3. **Restrict network access**:

   - Don't expose directly to the internet
   - Use firewall rules to limit access
   - Consider VPN or authentication layer

4. **Keep dependencies updated**:

   ```bash
   pip install --upgrade -r requirements.txt
   ```

5. **Monitor logs** for suspicious activity

### Development

1. **Never commit** `.secret_key` file (it's in .gitignore)
2. **Test security features** before deploying
3. **Review all user inputs** for proper validation

## Reporting Security Issues

If you discover a security vulnerability, please:

1. **Do NOT** open a public GitHub issue
2. Email the maintainer privately
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Compliance

This application implements security measures aligned with:

- OWASP Top 10 Web Application Security Risks
- CWE (Common Weakness Enumeration) standards
- SANS Top 25 Most Dangerous Software Errors

## Changelog

### 2026-01-01

- Fixed Path Traversal vulnerability (CWE-22)
- Fixed SSRF vulnerability (CWE-918)
- Fixed Command Injection vulnerability (CWE-78)
- Improved SECRET_KEY generation (CWE-330)
- Added Content Security Policy
- Enhanced security headers
- Added URL validation
- Improved path validation

---

**Note**: Security is an ongoing process. Regular updates and audits are recommended.
