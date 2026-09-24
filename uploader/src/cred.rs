//! Credential parsing for the Pixel XL uploader.
//!
//! Based on `gotohp/core/googleauth.go::buildGooglePhotosCredential` and
//! `gotohp/core/configmanager.go::AddCredentials`: a credential is a
//! URL-encoded query string carrying at least `Email`, `Token` (the master
//! token), and `androidId`.

/// Photos app id used for auth (`gotohp/core/googleauth.go`).
pub const PHOTOS_APP: &str = "com.google.android.apps.photos";
/// Default language when the credential has no `lang`.
pub const DEFAULT_LANG: &str = "en_US";

/// A parsed Google Photos credential.
#[derive(Clone)]
pub struct Credential {
    /// Account email (`Email`).
    pub email: String,
    /// Hex Android id (`androidId`).
    pub android_id: String,
    /// Language (`lang` or `DEFAULT_LANG`).
    pub lang: String,
    /// All original pairs, preserved for the auth request (includes `Token`).
    pub pairs: Vec<(String, String)>,
}

/// Parse a raw credential query string.
///
/// # Errors
///
/// Returns an error when the string is not a query string or when `Email`,
/// `Token`, or `androidId` are missing.
pub fn parse_credential(raw: &str) -> Result<Credential, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.contains('=') {
        return Err("credential must be a query string with Email and Token".to_string());
    }
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(trimmed.as_bytes())
        .into_owned()
        .collect();
    let mut seen = std::collections::HashSet::new();
    if pairs.iter().any(|(key, _)| !seen.insert(key)) {
        return Err("duplicate credential field".into());
    }
    let mut email: Option<String> = None;
    let mut token: Option<String> = None;
    let mut android_id: Option<String> = None;
    let mut lang = DEFAULT_LANG.to_string();
    for (key, value) in &pairs {
        if key == "Email" && email.is_none() {
            email = Some(value.clone());
        } else if key == "Token" && token.is_none() {
            token = Some(value.clone());
        } else if key == "androidId" && android_id.is_none() {
            android_id = Some(value.clone());
        } else if key == "lang" && lang == DEFAULT_LANG && !value.trim().is_empty() {
            lang.clone_from(value);
        }
    }
    let Some(email) = email.filter(|v| !v.trim().is_empty()) else {
        return Err("credential missing Email".to_string());
    };
    let has_token = token.as_ref().is_some_and(|v| !v.trim().is_empty());
    if !has_token {
        return Err("credential missing Token".to_string());
    }
    let Some(android_id) = android_id.filter(|v| !v.trim().is_empty()) else {
        return Err("credential missing androidId".to_string());
    };
    if !email.contains('@') {
        return Err("credential Email is not an email address".to_string());
    }
    Ok(Credential {
        email,
        android_id,
        lang,
        pairs,
    })
}

/// Build the `POST https://android.googleapis.com/auth` form body.
///
/// Mirrors `gotohp/core/api.go::getAuthToken`: copies stored pairs, forces
/// the Photos app, and drops fields this minimal client does not support.
///
/// # Errors
///
/// Returns an error when the credential needs token binding (requires a
/// rooted device via the `gotohp` GUI/ADB flow) or when serialization fails.
pub fn auth_form_body(cred: &Credential) -> Result<String, String> {
    for (key, _) in &cred.pairs {
        if key == "token_binding_alias" {
            return Err(
                "credential needs token binding; attach it with gotohp (rooted device + ADB)"
                    .to_string(),
            );
        }
    }
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in &cred.pairs {
        if key == "it_caveat_types" || key == "assertion_jwt" || key == "token_binding_alias" {
            continue;
        }
        if key == "app" || key == "callerPkg" {
            continue;
        }
        serializer.append_pair(key, value);
    }
    serializer.append_pair("app", PHOTOS_APP);
    serializer.append_pair("callerPkg", PHOTOS_APP);
    Ok(serializer.finish())
}

/// Construct gotohp's `EmbeddedSetup` credential from an existing AAS token.
///
/// # Errors
/// Rejects invalid inputs or unavailable OS randomness; never includes inputs in errors.
pub fn from_aas(email: &str, token: &str) -> Result<Credential, String> {
    if !email.contains('@')
        || !token.starts_with("aas_et/")
        || email.contains(['\r', '\n'])
        || token.contains(['\r', '\n'])
    {
        return Err("invalid AAS credentials".into());
    }
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).map_err(|_| "Android ID randomness unavailable".to_string())?;
    let android_id = format!("{:016x}", u64::from_be_bytes(random));
    let signature = "24bb24c05e47e0aefa68a58a766179d9b613a600";
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    body.extend_pairs([
        ("Email", email), ("Token", token), ("androidId", &android_id),
        ("app", PHOTOS_APP), ("callerPkg", PHOTOS_APP),
        ("callerSig", signature), ("client_sig", signature),
        ("device_country", "us"), ("operatorCountry", "us"),
        ("google_play_services_version", "240913000"), ("lang", DEFAULT_LANG),
        ("oauth2_foreground", "1"), ("sdk_version", "33"), ("source", "android"),
        ("service", "oauth2:openid https://www.googleapis.com/auth/mobileapps.native https://www.googleapis.com/auth/photos.native"),
    ]);
    parse_credential(&body.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aas_uses_upstream_embedded_setup_form() {
        let cred = from_aas("test@example.invalid", "aas_et/test").unwrap();
        assert_eq!(cred.android_id.len(), 16);
        assert!(cred.android_id.bytes().all(|b| b.is_ascii_hexdigit()));
        let body = auth_form_body(&cred).unwrap();
        let fields: std::collections::HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(fields.get("Token").map(String::as_str), Some("aas_et/test"));
        assert_eq!(
            fields.get("client_sig").map(String::as_str),
            Some("24bb24c05e47e0aefa68a58a766179d9b613a600")
        );
        assert!(!fields.contains_key("EncryptedPasswd"));
        assert!(parse_credential("Email=a&Email=b&Token=secret&androidId=1").is_err());
    }

    #[test]
    fn parses_minimal_credential() {
        let cred = parse_credential("Email=a%40b.c&Token=t123&androidId=abc123").unwrap();
        assert_eq!(cred.email, "a@b.c");
        assert_eq!(cred.lang, DEFAULT_LANG);
        let body = auth_form_body(&cred).unwrap();
        assert!(body.contains("com.google.android.apps.photos"));
    }

    #[test]
    fn rejects_token_binding() {
        let cred =
            parse_credential("Email=a%40b.c&Token=t&androidId=x&token_binding_alias=y").unwrap();
        assert!(auth_form_body(&cred).is_err());
    }
}
