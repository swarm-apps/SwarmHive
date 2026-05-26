//! Permission check helper + `require_permission!` macro.
//!
//! Usage in handlers:
//!
//! ```ignore
//! use swarmhive_api_types::PermissionName;
//! use crate::require_permission;
//!
//! async fn publish(p: Principal, ...) -> Result<_, ApiError> {
//!     require_permission!(p, PermissionName::ReleasePublish, Scope::None)?;
//!     ...
//! }
//! ```
//!
//! Scope semantics:
//! - `Scope::None` on the principal covers any required scope (org-level
//!   role grants apply everywhere).
//! - `Scope::App(a)` on the principal only satisfies `Scope::App(a)`; it
//!   cannot satisfy org-level requirements.

use swarmhive_api_types::PermissionName;

use super::principal::{Principal, Scope};
use crate::error::ApiError;

/// Returns `Ok` iff the principal both holds `perm` and its scope covers
/// `required_scope`. Returns `ApiError::Forbidden` carrying the missing
/// permission name otherwise.
pub fn check(
    principal: &Principal,
    perm: PermissionName,
    required_scope: Scope,
) -> Result<(), ApiError> {
    if !principal.has(perm) {
        return Err(ApiError::Forbidden {
            required_permission: perm.as_str().to_string(),
        });
    }
    if !scope_covers(&principal.scope, &required_scope) {
        return Err(ApiError::Forbidden {
            required_permission: perm.as_str().to_string(),
        });
    }
    Ok(())
}

fn scope_covers(have: &Scope, need: &Scope) -> bool {
    match (have, need) {
        (Scope::None, _) => true,
        (Scope::App(a), Scope::App(b)) => a == b,
        (Scope::App(_), Scope::None) => false,
    }
}

#[macro_export]
macro_rules! require_permission {
    ($principal:expr, $perm:expr, $scope:expr) => {
        $crate::auth::permission::check(&$principal, $perm, $scope)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::principal::AuthMethod;
    use std::collections::HashSet;
    use uuid::Uuid;

    fn principal_with(perms: &[PermissionName], scope: Scope) -> Principal {
        Principal {
            user_id: Uuid::now_v7(),
            org_id: Uuid::now_v7(),
            scope,
            permissions: perms.iter().copied().collect::<HashSet<_>>(),
            auth_method: AuthMethod::Session {
                session_id: Uuid::now_v7(),
            },
        }
    }

    #[test]
    fn org_level_principal_can_act_on_any_app() {
        let p = principal_with(&[PermissionName::ReleasePublish], Scope::None);
        let app = Uuid::now_v7();
        assert!(check(&p, PermissionName::ReleasePublish, Scope::App(app)).is_ok());
        assert!(check(&p, PermissionName::ReleasePublish, Scope::None).is_ok());
    }

    #[test]
    fn app_scoped_principal_cannot_do_org_level() {
        let app = Uuid::now_v7();
        let p = principal_with(&[PermissionName::StorageManage], Scope::App(app));
        assert!(check(&p, PermissionName::StorageManage, Scope::None).is_err());
    }

    #[test]
    fn missing_permission_returns_forbidden_with_name() {
        let p = principal_with(&[PermissionName::AppRead], Scope::None);
        let err = check(&p, PermissionName::ReleasePublish, Scope::None).unwrap_err();
        match err {
            ApiError::Forbidden {
                required_permission,
            } => assert_eq!(required_permission, "release:publish"),
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }
}
