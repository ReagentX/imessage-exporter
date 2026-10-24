//! Authentication method for encrypted backups.

/// Authentication method for encrypted backups.
///
/// Use this to supply either:
/// - a user password (to be `PBKDF2`-derived),  
/// - a pre-derived key (hex-encoded), or  
/// - no authentication for unencrypted backups.
///
/// # Examples
///
/// ```no_run
/// use crabapple::backup::models::auth::Authentication;
///
/// // Password-based
/// let auth1 = Authentication::Password("my_password".to_string());
///
/// // Pre-derived key (hex)
/// let auth2 = Authentication::DerivedKey("abcdef0123456789...".to_string());
///
/// // No auth (unencrypted backup)
/// let auth_none = Authentication::None;
/// ```
#[derive(Debug, Clone)]
pub enum Authentication {
    /// Cleartext password provided by the user.
    Password(String),
    /// Pre-derived key (hex-encoded) to decrypt backup.
    DerivedKey(String),
    /// No authentication (for unencrypted backups).
    None,
}
