use std::{error::Error, fmt::Display};

use crate::auth::{
    password::PasswordService,
    repository::{
        AuthRepository, AuthUserRecord, AuthenticatedUserRecord, ClientDevice, CreateSessionError,
    },
    token::issue_access_token,
};

const SESSION_EXPIRES_IN_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AuthService {
    repository: AuthRepository,
    password_service: PasswordService,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
    pub client: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginByUserIdInput {
    pub user_id: String,
    pub password: String,
    pub client: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginOutput {
    pub user_id: String,
    pub username: String,
    pub session_id: String,
    pub access_token: String,
    pub client: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub id: i64,
    pub public_id: String,
    pub username: String,
    pub role_name: String,
    pub role_name_normalized: String,
}

impl AuthenticatedUser {
    pub fn can_manage_server(&self) -> bool {
        matches!(
            self.role_name_normalized.as_str(),
            "owner" | "admin" | "administrator"
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthServiceError {
    InvalidCredentials,
    DisabledUser,
    MissingPassword,
    MissingDeviceId,
    NewDeviceLoginDisabled,
    DeviceRevoked,
    Repository(String),
}

impl AuthService {
    pub fn new(repository: AuthRepository) -> Self {
        Self {
            repository,
            password_service: PasswordService,
        }
    }

    pub async fn authenticate_by_name(
        &self,
        input: LoginInput,
    ) -> Result<LoginOutput, AuthServiceError> {
        let user = self
            .repository
            .find_user_by_name(&input.username)
            .await
            .map_err(repository_error)?
            .ok_or(AuthServiceError::InvalidCredentials)?;

        self.complete_login(
            user,
            LoginSessionInput {
                password: input.password,
                client: input.client,
                device: input.device,
                device_id: input.device_id,
                version: input.version,
            },
        )
        .await
    }

    pub async fn authenticate_by_user_id(
        &self,
        input: LoginByUserIdInput,
    ) -> Result<LoginOutput, AuthServiceError> {
        let user = self
            .repository
            .find_user_by_public_id(&input.user_id)
            .await
            .map_err(repository_error)?
            .ok_or(AuthServiceError::InvalidCredentials)?;

        self.complete_login(
            user,
            LoginSessionInput {
                password: input.password,
                client: input.client,
                device: input.device,
                device_id: input.device_id,
                version: input.version,
            },
        )
        .await
    }

    async fn complete_login(
        &self,
        user: AuthUserRecord,
        input: LoginSessionInput,
    ) -> Result<LoginOutput, AuthServiceError> {
        validate_login_user(&self.password_service, &user, &input.password)?;
        let device_id = input
            .device_id
            .map(|device_id| device_id.trim().to_owned())
            .filter(|device_id| !device_id.is_empty())
            .ok_or(AuthServiceError::MissingDeviceId)?;
        let issued_token = issue_access_token();
        let session = self
            .repository
            .create_session(
                &user,
                &ClientDevice {
                    device_id: device_id.clone(),
                    device_name: input.device.clone(),
                    client_name: input.client.clone(),
                    client_version: input.version.clone(),
                },
                issued_token.hash,
                SESSION_EXPIRES_IN_DAYS,
            )
            .await
            .map_err(create_session_error)?;

        Ok(LoginOutput {
            user_id: user.public_id,
            username: user.username,
            session_id: session.public_id,
            access_token: issued_token.token,
            client: input.client,
            device_id: Some(device_id),
            device_name: input.device,
            version: input.version,
        })
    }

    pub async fn logout(&self, token: &str) -> Result<bool, AuthServiceError> {
        self.repository
            .revoke_session_by_token(token)
            .await
            .map_err(repository_error)
    }

    pub async fn authenticate_access_token(
        &self,
        token: &str,
    ) -> Result<AuthenticatedUser, AuthServiceError> {
        self.repository
            .find_active_user_by_token(token)
            .await
            .map_err(repository_error)?
            .map(authenticated_user_from_record)
            .ok_or(AuthServiceError::InvalidCredentials)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoginSessionInput {
    password: String,
    client: Option<String>,
    device: Option<String>,
    device_id: Option<String>,
    version: Option<String>,
}

impl Display for AuthServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCredentials => f.write_str("invalid username or password"),
            Self::DisabledUser => f.write_str("user is disabled"),
            Self::MissingPassword => f.write_str("user has no password configured"),
            Self::MissingDeviceId => f.write_str("device id is required"),
            Self::NewDeviceLoginDisabled => {
                f.write_str("user is not allowed to login from new devices")
            }
            Self::DeviceRevoked => f.write_str("device is revoked"),
            Self::Repository(message) => write!(f, "authentication repository error: {message}"),
        }
    }
}

impl Error for AuthServiceError {}

fn validate_login_user(
    password_service: &PasswordService,
    user: &AuthUserRecord,
    password: &str,
) -> Result<(), AuthServiceError> {
    if user.is_disabled {
        return Err(AuthServiceError::DisabledUser);
    }

    let Some(password_hash) = user.password_hash.as_deref() else {
        return Err(AuthServiceError::MissingPassword);
    };

    if !password_service.verify(password_hash, password) {
        return Err(AuthServiceError::InvalidCredentials);
    }

    Ok(())
}

fn repository_error(error: sqlx::Error) -> AuthServiceError {
    AuthServiceError::Repository(error.to_string())
}

fn create_session_error(error: CreateSessionError) -> AuthServiceError {
    match error {
        CreateSessionError::NewDeviceLoginDisabled => AuthServiceError::NewDeviceLoginDisabled,
        CreateSessionError::DeviceRevoked => AuthServiceError::DeviceRevoked,
        CreateSessionError::Database(error) => repository_error(error),
    }
}

fn authenticated_user_from_record(record: AuthenticatedUserRecord) -> AuthenticatedUser {
    AuthenticatedUser {
        id: record.id,
        public_id: record.public_id,
        username: record.username,
        role_name: record.role_name,
        role_name_normalized: record.role_name_normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_user_is_rejected_before_password_check() {
        let service = PasswordService;
        let user = AuthUserRecord {
            id: 1,
            public_id: "user-1".to_owned(),
            username: "admin".to_owned(),
            password_hash: Some(service.hash_password("secret")),
            is_disabled: true,
            allow_new_device_login: true,
        };

        let err = validate_login_user(&service, &user, "secret").unwrap_err();

        assert_eq!(err, AuthServiceError::DisabledUser);
    }

    #[test]
    fn valid_argon2_password_is_accepted() {
        let service = PasswordService;
        let user = AuthUserRecord {
            id: 1,
            public_id: "user-1".to_owned(),
            username: "admin".to_owned(),
            password_hash: Some(service.hash_password("secret")),
            is_disabled: false,
            allow_new_device_login: true,
        };

        assert!(validate_login_user(&service, &user, "secret").is_ok());
        assert_eq!(
            validate_login_user(&service, &user, "wrong").unwrap_err(),
            AuthServiceError::InvalidCredentials
        );
    }

    #[test]
    fn create_session_errors_preserve_policy_boundary() {
        assert_eq!(
            create_session_error(CreateSessionError::NewDeviceLoginDisabled),
            AuthServiceError::NewDeviceLoginDisabled
        );
        assert_eq!(
            create_session_error(CreateSessionError::DeviceRevoked),
            AuthServiceError::DeviceRevoked
        );
    }
}
