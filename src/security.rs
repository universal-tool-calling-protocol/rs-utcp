use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Security utilities for validating inputs and preventing common vulnerabilities.

/// Validates that a file path is safe and doesn't allow directory traversal.
/// 
/// # Arguments
/// * `path` - The path to validate
/// * `allowed_base` - Optional base directory that the path must be within
/// 
/// # Returns
/// Canonicalized path if valid, error otherwise
pub fn validate_file_path(path: &str, allowed_base: Option<&str>) -> Result<PathBuf> {
    let path_buf = PathBuf::from(path);
    
    // Prevent absolute paths from escaping the base
    if let Some(base) = allowed_base {
        let canon_path = std::fs::canonicalize(&path_buf)
            .map_err(|e| anyhow!("Failed to canonicalize path '{}': {}", path, e))?;
        
        let canon_base = std::fs::canonicalize(base)
            .map_err(|e| anyhow!("Failed to canonicalize base '{}': {}", base, e))?;
        
        if !canon_path.starts_with(canon_base) {
            return Err(anyhow!(
                "Path '{}' is outside allowed directory '{}'",
                path,
                base
            ));
        }
        
        Ok(canon_path)
    } else {
        // Just canonicalize without base restriction
        std::fs::canonicalize(&path_buf)
            .map_err(|e| anyhow!("Invalid path '{}': {}", path, e))
    }
}

/// Validates a command name against an allowlist.
/// This helps prevent command injection attacks.
/// 
/// # Arguments
/// * `command` - The command to validate
/// * `allowed_commands` - List of permitted command names or paths
/// 
/// # Returns
/// Ok if command is in allowlist, error otherwise
pub fn validate_command(command: &str, allowed_commands: &[&str]) -> Result<()> {
    // Check for shell metacharacters that could enable injection
    const DANGEROUS_CHARS: &[char] = &['|', '&', ';', '\n', '`', '$', '(', ')', '<', '>', '"', '\'', '\\'];
    
    if command.chars().any(|c| DANGEROUS_CHARS.contains(&c)) {
        return Err(anyhow!(
            "Command contains dangerous characters: '{}'",
            command
        ));
    }
    
    // Extract just the command name (first component of path)
    let path_buf = PathBuf::from(command);
    let cmd_name = path_buf
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);
    
    // Check against allowlist
    if !allowed_commands.is_empty() && !allowed_commands.contains(&cmd_name) {
        return Err(anyhow!(
            "Command '{}' is not in the allowed list. Permitted commands: {:?}",
            cmd_name,
            allowed_commands
        ));
    }
    
    Ok(())
}

/// Validates command arguments for potentially dangerous content.
/// 
/// # Arguments
/// * `args` - The arguments to validate
/// 
/// # Returns
/// Ok if arguments appear safe, error otherwise
pub fn validate_command_args(args: &[String]) -> Result<()> {
    for arg in args {
        // Check for shell injection patterns
        if arg.contains("&&") || arg.contains("||") || arg.contains(";") || arg.contains("|") {
            return Err(anyhow!(
                "Argument contains dangerous shell operators: '{}'",
                arg
            ));
        }
        
        // Check for command substitution
        if arg.contains("$(") || arg.contains("`") {
            return Err(anyhow!(
                "Argument contains command substitution: '{}'",
                arg
            ));
        }
    }
    
    Ok(())
}

/// Validates that a URL uses a secure protocol (https://, wss://, etc.)
/// 
/// # Arguments
/// * `url` - The URL to validate
/// * `require_tls` - Whether to enforce TLS/SSL
/// 
/// # Returns
/// Ok if URL is valid and secure, error otherwise
pub fn validate_url_security(url: &str, require_tls: bool) -> Result<()> {
    let url_lower = url.to_lowercase();
    
    if require_tls {
        if !(url_lower.starts_with("https://")
            || url_lower.starts_with("wss://")
            || url_lower.starts_with("grpcs://"))
        {
            return Err(anyhow!(
                "URL must use TLS/SSL (https://, wss://, grpcs://): '{}'",
                url
            ));
        }
    }
    
    // Warn about localhost/127.0.0.1 in production (but allow it)
    if url_lower.contains("localhost") || url_lower.contains("127.0.0.1") {
        // This is just informational - don't fail
        eprintln!("Warning: URL uses localhost/127.0.0.1: '{}'", url);
    }
    
    Ok(())
}

/// Validates the size of input data to prevent DoS attacks.
/// 
/// # Arguments
/// * `data` - The data to check
/// * `max_size` - Maximum allowed size in bytes
/// 
/// # Returns
/// Ok if data is within limits, error otherwise
pub fn validate_size_limit(data: &[u8], max_size: usize) -> Result<()> {
    if data.len() > max_size {
        return Err(anyhow!(
            "Data size {} bytes exceeds maximum allowed size {} bytes",
            data.len(),
            max_size
        ));
    }
    
    Ok(())
}

/// Validates a timeout value to ensure it's reasonable.
/// 
/// # Arguments
/// * `timeout_ms` - Timeout in milliseconds
/// * `max_timeout_ms` - Maximum allowed timeout
/// 
/// # Returns
/// Ok if timeout is within limits, error otherwise
pub fn validate_timeout(timeout_ms: u64, max_timeout_ms: u64) -> Result<()> {
    if timeout_ms == 0 {
        return Err(anyhow!("Timeout cannot be zero"));
    }
    
    if timeout_ms > max_timeout_ms {
        return Err(anyhow!(
            "Timeout {}ms exceeds maximum allowed {}ms",
            timeout_ms,
            max_timeout_ms
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_command_rejects_dangerous_chars() {
        let allowed = vec!["python3", "node"];
        
        assert!(validate_command("python3", &allowed).is_ok());
        assert!(validate_command("ls; rm -rf /", &[]).is_err());
        assert!(validate_command("cat /etc/passwd | grep root", &[]).is_err());
        assert!(validate_command("echo `whoami`", &[]).is_err());
        assert!(validate_command("cmd && evil", &[]).is_err());
    }

    #[test]
    fn test_validate_command_allowlist() {
        let allowed = vec!["python3", "node", "npm"];
        
        assert!(validate_command("python3", &allowed).is_ok());
        assert!(validate_command("node", &allowed).is_ok());
        assert!(validate_command("bash", &allowed).is_err());
        assert!(validate_command("/usr/bin/python3", &allowed).is_ok()); // Path is ok if basename matches
    }

    #[test]
    fn test_validate_command_args() {
        assert!(validate_command_args(&["--help".to_string()]).is_ok());
        assert!(validate_command_args(&["-v".to_string(), "file.txt".to_string()]).is_ok());
        
        assert!(validate_command_args(&["arg && evil".to_string()]).is_err());
        assert!(validate_command_args(&["$(whoami)".to_string()]).is_err());
        assert!(validate_command_args(&["`id`".to_string()]).is_err());
        assert!(validate_command_args(&["arg | grep".to_string()]).is_err());
    }

    #[test]
    fn test_validate_url_security() {
        assert!(validate_url_security("https://api.example.com", true).is_ok());
        assert!(validate_url_security("wss://ws.example.com", true).is_ok());
        assert!(validate_url_security("http://api.example.com", true).is_err());
        assert!(validate_url_security("http://api.example.com", false).is_ok());
    }

    #[test]
    fn test_validate_size_limit() {
        let data = vec![0u8; 1000];
        assert!(validate_size_limit(&data, 2000).is_ok());
        assert!(validate_size_limit(&data, 500).is_err());
    }

    #[test]
    fn test_validate_timeout() {
        assert!(validate_timeout(1000, 60000).is_ok());
        assert!(validate_timeout(0, 60000).is_err());
        assert!(validate_timeout(100000, 60000).is_err());
    }

    #[test]
    fn test_validate_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();
        
        // Create a test file
        let test_file = temp_path.join("test.txt");
        fs::write(&test_file, b"test").unwrap();
        
        // Valid path within base
        let result = validate_file_path(
            test_file.to_str().unwrap(),
            Some(temp_path.to_str().unwrap())
        );
        assert!(result.is_ok());
        
        // Path outside base should fail
        let outside_path = "/tmp/outside.txt";
        let result = validate_file_path(
            outside_path,
            Some(temp_path.to_str().unwrap())
        );
        // This will fail because /tmp/outside.txt doesn't exist or is outside temp_dir
        assert!(result.is_err());
    }
}
