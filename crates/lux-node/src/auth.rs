// SPDX-License-Identifier: GPL-2.0-or-later
//! Passwords, tokens, PINs, names and the attempt limiter.
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::error::NodeError;

const COMMON_PASSWORDS: &[&str] = &[
    "password", "password1", "12345678", "123456789", "1234567890", "qwertyuiop", "iloveyou1", "letmein123",
    "passw0rd12", "administrator", "password123", "0123456789", "abcdefghij", "welcome123", "openpolis1",
];

/// Names that cannot be members: they would collide with standard mailboxes, service paths or the system itself.
const RESERVED: &[&str] = &[
    "admin", "administrator", "root", "postmaster", "abuse", "hostmaster", "webmaster", "noreply", "no-reply", "info",
    "support", "security", "mail", "chat", "insta", "www", "ftp", "api", "static", "ns1", "ns2", "smtp", "imap", "pop",
    "matrix", "openpolis", "system", "staff", "parlamento", "banca", "zecca", "piazza", "mercatino", "accademia", "social",
];

pub fn hash_password(pw: &str) -> Result<String, NodeError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| NodeError::Db(format!("hashing: {e}")))
}

pub fn verify_password(stored: &str, pw: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(h) => Argon2::default().verify_password(pw.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

/// Spends the same time as a real verification (used when the user does not exist).
pub fn dummy_verify(pw: &str) {
    let _ = hash_password(pw);
}

pub fn validate_username(raw: &str) -> Result<String, NodeError> {
    let u = raw.trim().to_lowercase();
    let ok_chars = u.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !(3..=30).contains(&u.len()) || !ok_chars || u.starts_with(['-', '_']) || u.ends_with(['-', '_']) {
        return Err(NodeError::InvalidInput(
            "Il nome utente deve avere da 3 a 30 caratteri: lettere minuscole, numeri, «-» e «_».".into(),
        ));
    }
    if RESERVED.contains(&u.as_str()) {
        return Err(NodeError::InvalidInput("Questo nome utente è riservato.".into()));
    }
    Ok(u)
}

pub fn validate_password(pw: &str, username: &str) -> Result<(), NodeError> {
    if pw.chars().count() < 10 {
        return Err(NodeError::InvalidInput("La password deve avere almeno 10 caratteri.".into()));
    }
    if pw.len() > 200 {
        return Err(NodeError::InvalidInput("La password è troppo lunga.".into()));
    }
    let low = pw.to_lowercase();
    if COMMON_PASSWORDS.contains(&low.as_str()) || low == username {
        return Err(NodeError::InvalidInput("Scegli una password meno prevedibile.".into()));
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

/// 256-bit session token, hex-encoded.
pub fn new_token() -> String {
    hex(&random_bytes::<32>())
}

pub fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

pub fn hmac_hex(secret: &[u8], msg: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    m.update(msg.as_bytes());
    hex(&m.finalize().into_bytes())
}

/// Uniform 6-digit PIN (rejection sampling: no modulo bias).
pub fn random_pin() -> String {
    loop {
        let v = u32::from_le_bytes(random_bytes::<4>());
        if v < 4_294_000_000 {
            return format!("{:06}", v % 1_000_000);
        }
    }
}

pub fn valid_pin_format(p: &str) -> bool {
    p.len() == 6 && p.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------- attempt limiter (failures only)
pub fn is_locked(conn: &Connection, key: &str, limit: i64, now: i64) -> Result<bool, NodeError> {
    let row: Option<(i64, i64)> = conn
        .query_row("SELECT count, reset_at FROM throttle WHERE key = ?1", params![key], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(matches!(row, Some((count, reset)) if reset > now && count >= limit))
}

pub fn record_failure(conn: &Connection, key: &str, window: i64, now: i64) -> Result<(), NodeError> {
    let row: Option<i64> =
        conn.query_row("SELECT reset_at FROM throttle WHERE key = ?1", params![key], |r| r.get(0)).optional()?;
    match row {
        Some(reset) if reset > now => {
            conn.execute("UPDATE throttle SET count = count + 1 WHERE key = ?1", params![key])?;
        }
        _ => {
            conn.execute(
                "INSERT INTO throttle (key, count, reset_at) VALUES (?1, 1, ?2)
                 ON CONFLICT(key) DO UPDATE SET count = 1, reset_at = ?2",
                params![key, now + window],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_have_six_digits_and_are_not_all_the_same() {
        let pins: Vec<String> = (0..200).map(|_| random_pin()).collect();
        assert!(pins.iter().all(|p| valid_pin_format(p)));
        assert!(pins.iter().any(|p| p != &pins[0]));
    }

    #[test]
    fn names_and_passwords() {
        assert_eq!(validate_username("  Fulano_1 ").unwrap(), "fulano_1");
        assert!(validate_username("postmaster").is_err());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("-ciao").is_err());
        assert!(validate_username("con spazio").is_err());
        assert!(validate_password("corta", "x").is_err());
        assert!(validate_password("password123", "x").is_err());
        assert!(validate_password("una frase lunga e strana", "x").is_ok());
    }

    #[test]
    fn password_hashes_verify() {
        let h = hash_password("una frase lunga e strana").unwrap();
        assert!(verify_password(&h, "una frase lunga e strana"));
        assert!(!verify_password(&h, "un'altra frase"));
        assert!(h.starts_with("$argon2"));
    }
}
