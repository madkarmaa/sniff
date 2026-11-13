use crate::Gpapi;

/// What a logged-in client needs to resume on another process or machine.
///
/// Every check-in registers a new device on the Google account, so callers
/// that log in repeatedly (serverless workers, CLIs) should persist this after
/// [`Gpapi::login`] and hand it back with [`Gpapi::restore_session`]. The
/// device part (`gsf_id`, the two device tokens) stays valid for a long time;
/// the session part (`auth_token`, `dfe_cookie`) expires and is renewed with
/// [`Gpapi::reauthenticate`], which keeps the registered device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    pub gsf_id: i64,
    pub device_checkin_consistency_token: Option<String>,
    pub device_config_token: String,
    pub auth_token: Option<String>,
    pub dfe_cookie: Option<String>,
}

impl Gpapi {
    /// The state to persist, or `None` before the device is registered.
    #[must_use]
    pub fn session_state(&self) -> Option<SessionState> {
        Some(SessionState {
            gsf_id: self.gsf_id?,
            device_checkin_consistency_token: self.device_checkin_consistency_token.clone(),
            device_config_token: self.device_config_token.clone()?,
            auth_token: self.auth_token.clone(),
            dfe_cookie: self.dfe_cookie.clone(),
        })
    }

    /// Resume a session saved with [`Gpapi::session_state`].
    ///
    /// The state must come from a client with the same email, device and
    /// ABIs; Google ties the registered device to all three.
    pub fn restore_session(&mut self, state: SessionState) {
        self.gsf_id = Some(state.gsf_id);
        self.device_checkin_consistency_token = state.device_checkin_consistency_token;
        self.device_config_token = Some(state.device_config_token);
        self.auth_token = state.auth_token;
        self.dfe_cookie = state.dfe_cookie;
    }

    /// Whether a device is registered, so [`Gpapi::reauthenticate`] can skip
    /// the check-in.
    #[must_use]
    pub const fn has_registered_device(&self) -> bool {
        self.gsf_id.is_some() && self.device_config_token.is_some()
    }
}

#[cfg(test)]
#[path = "tests/resume.rs"]
mod tests;
