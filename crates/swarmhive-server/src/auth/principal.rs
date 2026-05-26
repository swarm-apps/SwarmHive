//! Authenticated caller identity, derived once per request from the
//! cookie session (or, in a later proposal, a bearer token).
//!
//! In MVP `Principal.scope` is always `Scope::None` for session-derived
//! principals: app-level scoping is enforced through `user_role.scope_app_id`
//! at permission-check time once `add-app-release-artifact` introduces
//! scoped operations. PAT / API Token principals (added by
//! `add-pat-and-api-token`) will carry concrete scopes.

use std::collections::HashSet;

use swarmhive_api_types::PermissionName;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Org-level: principal can act on any resource the permission allows.
    None,
    /// Restricted to a single app.
    App(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    Session {
        session_id: Uuid,
    },
    /// Reserved for `add-pat-and-api-token`.
    Pat {
        token_id: Uuid,
    },
    /// Reserved for `add-pat-and-api-token`.
    ApiToken {
        token_id: Uuid,
        scope: Scope,
    },
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: Uuid,
    pub org_id: Uuid,
    pub scope: Scope,
    pub permissions: HashSet<PermissionName>,
    pub auth_method: AuthMethod,
}

impl Principal {
    pub fn has(&self, perm: PermissionName) -> bool {
        self.permissions.contains(&perm)
    }
}
