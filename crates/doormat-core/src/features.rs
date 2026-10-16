use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, MatchKind};

use crate::store::CellStore;
use crate::types::*;

// ---------- Aho-Corasick header automatons ----------

static PASSWORD_PATTERNS: &[&str] = &[
    "password",
    "pwd",
    "pass",
    "passwd",
    "passphrase",
    "passcode",
    "passwort",
    "contraseña",
    "motdepasse",
    "senha",
    "пароль",
    "パスワード",
    "密码",
    "비밀번호",
    "wachtwoord",
    "hasło",
    "lösenord",
    "secretkey",
    "secret",
    "apikey",
    "apikeypassword",
    "token",
    "accesstoken",
    "authtoken",
    "totp",
    "otpsecret",
];

static USERNAME_PATTERNS: &[&str] = &[
    "username",
    "user",
    "login",
    "email",
    "emailaddress",
    "account",
    "accountname",
    "userid",
    "usuario",
    "benutzername",
    "utilisateur",
    "utente",
    "gebruiker",
    "użytkownik",
    "användare",
];

static URL_PATTERNS: &[&str] = &[
    "url", "uri", "link", "website", "endpoint", "server", "host", "site", "homepage", "webpage",
    "href", "baseurl",
];

fn build_ac(patterns: &[&str]) -> AhoCorasick {
    AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostLongest)
        .build(patterns)
        .expect("valid patterns")
}

static PASSWORD_AC: LazyLock<AhoCorasick> = LazyLock::new(|| build_ac(PASSWORD_PATTERNS));

static USERNAME_AC: LazyLock<AhoCorasick> = LazyLock::new(|| build_ac(USERNAME_PATTERNS));

static URL_AC: LazyLock<AhoCorasick> = LazyLock::new(|| build_ac(URL_PATTERNS));

/// Check if `text` exactly matches any pattern in the automaton.
/// Aho-Corasick finds substrings; we need full-string match.
fn ac_exact_match(ac: &AhoCorasick, text: &str) -> bool {
    ac_scan(ac, text).0
}

/// One pass over `text`: whether a pattern matches the whole text, and
/// whether any pattern occurs in it at all.
fn ac_scan(ac: &AhoCorasick, text: &str) -> (bool, bool) {
    let mut any = false;
    for mat in ac.find_iter(text) {
        any = true;
        if mat.start() == 0 && mat.end() == text.len() {
            return (true, true);
        }
    }
    (false, any)
}

// ---------- Compound labels ----------

/// Split an identifier-style label into lowercase parts at `_`, `.`, `-` and
/// camelCase boundaries: `loginPassword` -> `login`, `password`.
fn label_parts(label: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut prev_lower = false;
    for c in label.chars() {
        if matches!(c, '_' | '.' | '-') {
            parts.push(String::new());
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            parts.push(String::new());
        }
        prev_lower = c.is_lowercase();
        parts
            .last_mut()
            .expect("parts is never empty")
            .extend(c.to_lowercase());
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// True when `value` looks like a variable name: letters, digits and `_ . -`
/// only, starting with a letter, in at least two `_`/`.`-separated parts
/// (`MYSQL_DATABASE`, `database__connection__user`). A trailing `:` or `=` is
/// ignored.
pub fn is_variable_name(value: &str) -> bool {
    let label = value.trim().trim_end_matches([':', '=']);
    label.len() <= COMPOUND_LABEL_MAX_LEN
        && label.chars().next().is_some_and(char::is_alphabetic)
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && label.split(['_', '.']).filter(|p| !p.is_empty()).count() >= 2
}

/// True when `value` is an identifier-style compound label whose final part
/// names a credential: `MYSQL_ROOT_PASSWORD`, `login_password`,
/// `mail__options__auth__pass`, `ME_CONFIG_MONGODB_ADMINPASSWORD`.
///
/// Only labels without whitespace and made of letters, digits and `_ . -` are
/// considered (a trailing `:` or `=` is ignored), so prose and URLs never
/// match. The final part must be a password keyword, or end in one of
/// `COMPOUND_SUFFIXES`; a credential word followed by another part is a
/// different thing (`PASSWORD_FILE` is a path, `PASSWORD_HASH` a hash).
pub fn is_compound_credential_label(value: &str) -> bool {
    let label = value.trim().trim_end_matches([':', '=']);
    if label.is_empty()
        || label.len() > COMPOUND_LABEL_MAX_LEN
        || !label
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return false;
    }
    let parts = label_parts(label);
    let Some(last) = parts.last() else {
        return false;
    };
    (parts.len() >= 2 && ac_exact_match(&PASSWORD_AC, last))
        || COMPOUND_SUFFIXES
            .iter()
            .any(|s| last.len() > s.len() && last.ends_with(s))
}

/// True when `value` is an identifier-style label whose final part names key
/// material (`WORDPRESS_AUTH_KEY`, `WORDPRESS_NONCE_SALT`; `KEY_PARTS`).
pub fn is_compound_key_label(value: &str) -> bool {
    is_variable_name(value)
        && label_parts(value.trim().trim_end_matches([':', '=']))
            .last()
            .is_some_and(|last| KEY_PARTS.contains(&last.as_str()))
}

/// True when `value` looks like key material: at least `KEY_MATERIAL_MIN_LEN`
/// characters mixing upper case, lower case and digits, with entropy of at
/// least `ENTROPY_THRESHOLD`.
pub fn is_key_material(value: &str) -> bool {
    value.chars().count() >= KEY_MATERIAL_MIN_LEN
        && value.chars().any(char::is_uppercase)
        && value.chars().any(char::is_lowercase)
        && value.chars().any(|c| c.is_ascii_digit())
        && compute_entropy(value) >= ENTROPY_THRESHOLD
}

/// Split a one-line assignment (`NAME=value`, `NAME: value`) whose name is a
/// compound credential or key label; returns `(name, value)`. The value must
/// be non-empty, and for a key label it must be key material.
pub fn secret_assignment(text: &str) -> Option<(&str, &str)> {
    let at = text.find(['=', ':'])?;
    let (name, rest) = (text[..at].trim(), text[at + 1..].trim());
    let value = rest.trim_matches(['"', '\'']);
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    let credential = is_compound_credential_label(name) && is_variable_name(name);
    let key = is_compound_key_label(name) && is_key_material(value);
    (credential || key).then_some((name, value))
}

/// True when `value` is a money amount or measurement rather than a secret: an
/// optionally signed number with a decimal part or a leading sign
/// (`-12345.67`, `1,250.00`, `+3`). Plain digit strings such as PINs are not
/// amounts.
pub fn is_numeric_amount(value: &str) -> bool {
    let v = value.trim();
    let (signed, body) = match v.strip_prefix(['-', '+']) {
        Some(rest) => (true, rest),
        None => (false, v),
    };
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ',');
    digits(whole)
        && (fraction.is_empty() || fraction.chars().all(|c| c.is_ascii_digit()))
        && (signed || !fraction.is_empty())
}

/// True when a normalized value is a switch (`yes`, `true`, `0`, ...), which is
/// never a credential.
pub fn is_boolean_literal(normalized: &str) -> bool {
    BOOLEAN_LITERALS.contains(&normalized)
}

// ---------- Normalize ----------

/// Lowercase, strip, collapse [\s_\-:=]+ to nothing.
pub fn normalize(value: &str) -> String {
    let mut buf = String::with_capacity(value.len());
    normalize_into(value, &mut buf);
    buf
}

/// Normalize into an existing buffer, avoiding allocation.
#[inline]
fn normalize_into(value: &str, buf: &mut String) {
    buf.clear();
    if value.is_ascii() {
        // Fast path: all ASCII, skip char decoding
        for &b in value.as_bytes() {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' | b'_' | b'-' | b':' | b'=' => {}
                b'A'..=b'Z' => buf.push((b + 32) as char),
                _ => buf.push(b as char),
            }
        }
    } else {
        // Slow path: non-ASCII (multilingual headers)
        for ch in value.chars() {
            if matches!(ch, ' ' | '\t' | '\n' | '\r' | '_' | '-' | ':' | '=') {
                continue;
            }
            for lc in ch.to_lowercase() {
                buf.push(lc);
            }
        }
    }
}

/// Fused normalize + character-class flag detection in a single pass.
/// Writes the normalized value into `norm_buf` and returns the flags.
#[inline]
fn normalize_and_flags(value: &str, norm_buf: &mut String) -> u16 {
    norm_buf.clear();

    if value.is_empty() {
        return FLAG_IS_EMPTY;
    }

    let bytes = value.as_bytes();
    let mut flags: u16 = 0;
    let mut has_upper = false;
    let mut has_lower = false;
    let mut has_digit = false;
    let mut has_special = false;
    let mut all_alpha = true;
    let mut all_numeric = true;
    let mut has_colon = false;

    if value.is_ascii() {
        // Fast path: single pass over ASCII bytes
        norm_buf.reserve(bytes.len());
        for &b in bytes {
            match b {
                b'A'..=b'Z' => {
                    has_upper = true;
                    norm_buf.push((b + 32) as char);
                }
                b'a'..=b'z' => {
                    has_lower = true;
                    norm_buf.push(b as char);
                }
                b'0'..=b'9' => {
                    has_digit = true;
                    all_alpha = false;
                    norm_buf.push(b as char);
                }
                b' ' | b'\t' | b'\n' | b'\r' | b'_' | b'-' | b'=' => {
                    // Strip from normalized output
                    all_alpha = false;
                    all_numeric = false;
                }
                b':' => {
                    has_colon = true;
                    // Strip from normalized output
                    all_alpha = false;
                    all_numeric = false;
                }
                _ => {
                    // Non-alphanumeric, non-whitespace → special
                    has_special = true;
                    all_alpha = false;
                    all_numeric = false;
                    norm_buf.push(b as char);
                }
            }
        }
    } else {
        // Slow path: non-ASCII values (multilingual)
        for ch in value.chars() {
            if ch.is_uppercase() {
                has_upper = true;
            } else if ch.is_lowercase() {
                has_lower = true;
            }
            if ch.is_ascii_digit() {
                has_digit = true;
            } else {
                all_numeric = false;
            }
            if !ch.is_alphanumeric() && !ch.is_whitespace() {
                has_special = true;
            }
            if !ch.is_alphabetic() {
                all_alpha = false;
            }
            if ch == ':' {
                has_colon = true;
            }
            // Normalize: strip separators, lowercase
            if matches!(ch, ' ' | '\t' | '\n' | '\r' | '_' | '-' | ':' | '=') {
                continue;
            }
            for lc in ch.to_lowercase() {
                norm_buf.push(lc);
            }
        }
    }

    if has_upper {
        flags |= FLAG_HAS_UPPER;
    }
    if has_lower {
        flags |= FLAG_HAS_LOWER;
    }
    if has_digit {
        flags |= FLAG_HAS_DIGIT;
    }
    if has_special {
        flags |= FLAG_HAS_SPECIAL;
    }
    if value.len() < MIN_CANDIDATE_LENGTH {
        flags |= FLAG_IS_SHORT;
    }
    if all_numeric && has_digit {
        flags |= FLAG_IS_NUMERIC;
    }
    if all_alpha && (has_upper || has_lower) {
        flags |= FLAG_IS_ALPHA;
    }
    if has_colon {
        flags |= FLAG_HAS_COLON;
    }

    flags
}

// ---------- Entropy ----------

/// Shannon entropy: -Σ (count/len) * log2(count/len)
pub fn compute_entropy(text: &str) -> f32 {
    let len = text.len();
    if len == 0 {
        return 0.0;
    }

    let mut counts = [0u32; 256];
    for &b in text.as_bytes() {
        counts[b as usize] += 1;
    }

    let len_f = len as f32;
    let mut entropy = 0.0_f32;
    for &count in &counts {
        if count > 0 {
            let p = count as f32 / len_f;
            entropy -= p * p.log2();
        }
    }

    entropy
}

// ---------- Feature precomputation ----------

/// Single-pass feature computation over all cells in the store.
///
/// Uses `normalize_and_flags()` to fuse normalization and character-class
/// detection into a single pass, avoiding the intermediate `to_lowercase()`
/// allocation that the standalone `normalize()` produces.
pub fn precompute_features(store: &mut CellStore) {
    let entropy_prefilter = FLAG_HAS_UPPER | FLAG_HAS_LOWER | FLAG_HAS_DIGIT;

    // Swap pattern: take the slot out, use it as the normalize buffer,
    // then put it back. Avoids per-cell allocation since the buffer
    // capacity grows once and is reused across cells.
    let mut norm_buf = String::new();

    for i in 0..store.len() {
        let value = store.get_value(i);
        let formula = store.get_formula(i);
        let comment = store.get_comment(i);

        // Fused normalize + flag detection (single pass, buffer reuse)
        let mut flags = normalize_and_flags(value, &mut norm_buf);

        // Formula flag
        if !formula.is_empty() {
            flags |= FLAG_HAS_FORMULA;
        }

        // Comment flag
        if !comment.is_empty() {
            flags |= FLAG_HAS_COMMENT;
        }

        // Header detection against normalized value (Aho-Corasick exact match)
        // One automaton pass gives both the exact label match and whether a
        // credential word occurs at all; compound labels are checked only then.
        let (exact, credential_word) = ac_scan(&PASSWORD_AC, &norm_buf);
        if credential_word {
            flags |= FLAG_HAS_CREDENTIAL_WORD;
        }
        if exact || (credential_word && is_compound_credential_label(value)) {
            flags |= FLAG_IS_PASSWORD_HEADER;
        } else if (norm_buf.ends_with("key") || norm_buf.ends_with("salt"))
            && is_compound_key_label(value)
        {
            flags |= FLAG_IS_KEY_LABEL;
        }
        if ac_exact_match(&USERNAME_AC, &norm_buf) {
            flags |= FLAG_IS_USERNAME_HEADER;
        }
        if ac_exact_match(&URL_AC, &norm_buf) {
            flags |= FLAG_IS_URL_HEADER;
        }

        // Entropy only when len >= ENTROPY_MIN_LENGTH; skips float math for most cells
        // AND has upper + lower + digit
        let ent = if value.len() >= ENTROPY_MIN_LENGTH
            && (flags & entropy_prefilter) == entropy_prefilter
        {
            compute_entropy(value)
        } else {
            0.0
        };

        store.feature_flags[i] = flags;
        store.entropy[i] = ent;

        // Swap the reusable buffer into the store slot, take the old
        // (empty) slot as the new buffer. This avoids allocation since
        // the buffer retains its capacity across iterations.
        std::mem::swap(&mut store.normalized_values[i], &mut norm_buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::RawCell;

    // ---------- normalize ----------

    #[test]
    fn normalize_spaces() {
        assert_eq!(normalize("Pass Word"), "password");
    }

    #[test]
    fn normalize_underscore() {
        assert_eq!(normalize("PASS_WORD"), "password");
    }

    #[test]
    fn normalize_dash_colon() {
        assert_eq!(normalize("pass-word:"), "password");
    }

    #[test]
    fn normalize_equals() {
        assert_eq!(normalize("key=value"), "keyvalue");
    }

    // ---------- header sets ----------

    #[test]
    fn password_headers_contains_password() {
        assert!(ac_exact_match(&PASSWORD_AC, "password"));
    }

    #[test]
    fn password_headers_contains_multilingual() {
        assert!(ac_exact_match(&PASSWORD_AC, "contraseña"));
        assert!(ac_exact_match(&PASSWORD_AC, "passwort"));
        assert!(ac_exact_match(&PASSWORD_AC, "пароль"));
        assert!(ac_exact_match(&PASSWORD_AC, "パスワード"));
        assert!(ac_exact_match(&PASSWORD_AC, "密码"));
        assert!(ac_exact_match(&PASSWORD_AC, "비밀번호"));
        assert!(ac_exact_match(&PASSWORD_AC, "wachtwoord"));
        assert!(ac_exact_match(&PASSWORD_AC, "hasło"));
        assert!(ac_exact_match(&PASSWORD_AC, "lösenord"));
        assert!(ac_exact_match(&PASSWORD_AC, "senha"));
    }

    #[test]
    fn password_headers_contains_totp_secrets() {
        assert!(ac_exact_match(&PASSWORD_AC, "totp"));
        assert!(ac_exact_match(&PASSWORD_AC, "otpsecret"));
        assert!(!ac_exact_match(&PASSWORD_AC, "otp"));
    }

    #[test]
    fn compound_labels_ending_in_a_credential_word() {
        for label in [
            "MYSQL_ROOT_PASSWORD",
            "login_password",
            "loginPassword",
            "mail__options__auth__pass",
            "ME_CONFIG_MONGODB_ADMINPASSWORD",
            "ME_CONFIG_SITE_COOKIESECRET",
            "app.pwd",
            "DB_PASSWORD:",
        ] {
            assert!(is_compound_credential_label(label), "{label}");
        }
    }

    #[test]
    fn compound_labels_that_are_not_credentials() {
        for label in [
            "POSTGRES_PASSWORD_FILE",
            "MARIADB_ROOT_PASSWORD_HASH",
            "ME_CONFIG_SITE_SSL_KEY_PATH",
            "bypass",
            "compass",
            "Change Password",
            "https://example.test/?password",
            "PASSWORD",
            "",
        ] {
            assert!(!is_compound_credential_label(label), "{label}");
        }
    }

    #[test]
    fn variable_names() {
        assert!(
            is_variable_name("MYSQL_DATABASE") && is_variable_name("database__connection__user")
        );
        assert!(
            !is_variable_name("PGDATA")
                && !is_variable_name("Variable")
                && !is_variable_name("Tr0ub4dor!9")
        );
    }

    #[test]
    fn key_labels_and_key_material() {
        assert!(
            is_compound_key_label("WORDPRESS_AUTH_KEY")
                && is_compound_key_label("WORDPRESS_NONCE_SALT")
        );
        assert!(
            !is_compound_key_label("ME_CONFIG_SITE_SSL_KEY_PATH") && !is_compound_key_label("KEY")
        );
        assert!(is_key_material("q8Zr2Lm9Xa7Pk3Vw"));
        assert!(!is_key_material("user_id") && !is_key_material("ABCDEFGH1234567890XY"));
    }

    #[test]
    fn secret_assignments() {
        assert_eq!(
            secret_assignment("ME_CONFIG_MONGODB_ADMINPASSWORD=Tr0ub4dor!9"),
            Some(("ME_CONFIG_MONGODB_ADMINPASSWORD", "Tr0ub4dor!9"))
        );
        assert_eq!(
            secret_assignment("WORDPRESS_AUTH_KEY=q8Zr2Lm9Xa7Pk3Vw"),
            Some(("WORDPRESS_AUTH_KEY", "q8Zr2Lm9Xa7Pk3Vw"))
        );
        assert_eq!(secret_assignment("WORDPRESS_AUTH_KEY=short"), None);
        assert_eq!(
            secret_assignment("POSTGRES_PASSWORD_FILE=/run/secrets/pw"),
            None
        );
        assert_eq!(secret_assignment("Password reset = done today"), None);
    }

    #[test]
    fn numeric_amounts() {
        assert!(
            is_numeric_amount("-99999.9999")
                && is_numeric_amount("1,250.00")
                && is_numeric_amount("+3")
        );
        assert!(
            !is_numeric_amount("4821")
                && !is_numeric_amount("Tr0ub4dor")
                && !is_numeric_amount("1.2.3")
        );
    }

    #[test]
    fn boolean_literals() {
        assert!(is_boolean_literal("yes") && is_boolean_literal("true") && is_boolean_literal("0"));
        assert!(!is_boolean_literal("yes123") && !is_boolean_literal("Tr0ub4dor"));
    }

    #[test]
    fn password_headers_contains_motdepasse() {
        // "mot de passe" normalizes to "motdepasse"
        assert!(ac_exact_match(&PASSWORD_AC, "motdepasse"));
    }

    #[test]
    fn username_headers_basic() {
        assert!(ac_exact_match(&USERNAME_AC, "username"));
        assert!(ac_exact_match(&USERNAME_AC, "email"));
        assert!(ac_exact_match(&USERNAME_AC, "usuario"));
        assert!(ac_exact_match(&USERNAME_AC, "benutzername"));
    }

    #[test]
    fn url_headers_basic() {
        assert!(ac_exact_match(&URL_AC, "url"));
        assert!(ac_exact_match(&URL_AC, "endpoint"));
        assert!(ac_exact_match(&URL_AC, "host"));
    }

    // ---------- entropy ----------

    #[test]
    fn entropy_low_for_repeated() {
        let e = compute_entropy("aaaa");
        assert!(e < 1.0, "entropy of 'aaaa' should be < 1.0, got {e}");
    }

    #[test]
    fn entropy_high_for_mixed() {
        let e = compute_entropy("aB3$xY9!");
        assert!(e >= 3.0, "entropy of 'aB3$xY9!' should be >= 3.0, got {e}");
    }

    #[test]
    fn entropy_zero_for_empty() {
        assert_eq!(compute_entropy(""), 0.0);
    }

    // ---------- precompute_features ----------

    fn make_store(cells: Vec<(&str, &str)>) -> CellStore {
        let raws: Vec<RawCell> = cells
            .into_iter()
            .enumerate()
            .map(|(i, (v, f))| RawCell {
                row: i as u32,
                col: 0,
                value: v.into(),
                formula: f.into(),
                comment: String::new(),
                sheet_name: "Sheet1".into(),
                is_merged_origin: false,
            })
            .collect();
        let mut store = CellStore::from_raw(raws);
        precompute_features(&mut store);
        store
    }

    #[test]
    fn precompute_password_header() {
        let store = make_store(vec![("Password", "")]);
        assert_ne!(store.feature_flags[0] & FLAG_IS_PASSWORD_HEADER, 0);
    }

    #[test]
    fn precompute_short_no_entropy() {
        let store = make_store(vec![("abc", "")]);
        assert_eq!(store.entropy[0], 0.0);
    }

    #[test]
    fn precompute_upper_only() {
        let store = make_store(vec![("ABC", "")]);
        assert_ne!(store.feature_flags[0] & FLAG_HAS_UPPER, 0);
        assert_eq!(store.feature_flags[0] & FLAG_HAS_LOWER, 0);
    }

    #[test]
    fn precompute_entropy_computed_when_eligible() {
        // "Abc12345" has upper + lower + digit, len == 8 >= ENTROPY_MIN_LENGTH
        let store = make_store(vec![("Abc12345", "")]);
        let flags = store.feature_flags[0];
        assert_ne!(flags & FLAG_HAS_UPPER, 0);
        assert_ne!(flags & FLAG_HAS_LOWER, 0);
        assert_ne!(flags & FLAG_HAS_DIGIT, 0);
        assert!(store.entropy[0] > 0.0, "entropy should be computed");
    }

    #[test]
    fn precompute_entropy_skipped_missing_lower() {
        // "ABCD1234" has upper + digit but no lower → entropy not computed
        let store = make_store(vec![("ABCD1234", "")]);
        assert_eq!(store.entropy[0], 0.0);
    }

    #[test]
    fn precompute_formula_flag() {
        let store = make_store(vec![("result", "=SUM(A1)")]);
        assert_ne!(store.feature_flags[0] & FLAG_HAS_FORMULA, 0);
    }

    #[test]
    fn precompute_no_formula_flag_when_empty() {
        let store = make_store(vec![("result", "")]);
        assert_eq!(store.feature_flags[0] & FLAG_HAS_FORMULA, 0);
    }

    #[test]
    fn precompute_normalized_value_stored() {
        let store = make_store(vec![("Pass Word", "")]);
        assert_eq!(store.normalized_values[0], "password");
    }

    #[test]
    fn precompute_empty_value() {
        let store = make_store(vec![("", "")]);
        assert_ne!(store.feature_flags[0] & FLAG_IS_EMPTY, 0);
        assert_eq!(store.entropy[0], 0.0);
    }
}
