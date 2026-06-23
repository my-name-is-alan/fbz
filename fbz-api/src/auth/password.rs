use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordVerifier};

#[derive(Clone, Debug, Default)]
pub struct PasswordService;

impl PasswordService {
    pub fn verify(&self, password_hash: &str, password: &str) -> bool {
        let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
            return false;
        };

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }

    pub fn hash_password(&self, password: &str) -> String {
        let salt = SaltString::generate(&mut OsRng);

        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .expect("argon2 password hash should be created")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_argon2_password_hash() {
        let service = PasswordService;
        let hash = service.hash_password("correct horse battery staple");

        assert!(service.verify(&hash, "correct horse battery staple"));
        assert!(!service.verify(&hash, "wrong password"));
    }

    #[test]
    fn rejects_invalid_hash_format() {
        let service = PasswordService;

        assert!(!service.verify("plain:password", "password"));
    }
}
