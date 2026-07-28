//! One error-category vocabulary shared by metrics, engine health (storage
//! circuit), and user-facing serialization.

use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    Captcha,
    CloudflareCaptcha,
    RecaptchaCaptcha,
    AccessDenied,
    RateLimited,
    Timeout,
    QueueExpired,
    Parse,
    Unexpected,
    /// Engine never responded within the aggregation deadline (no `EngineError`).
    Unresponsive,
}

impl ErrorCategory {
    /// Stable lowercase identifier: metrics label value and storage column value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCategory::Captcha => "captcha",
            ErrorCategory::CloudflareCaptcha => "cloudflare_captcha",
            ErrorCategory::RecaptchaCaptcha => "recaptcha_captcha",
            ErrorCategory::AccessDenied => "access_denied",
            ErrorCategory::RateLimited => "rate_limited",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::QueueExpired => "queue_expired",
            ErrorCategory::Parse => "parse",
            ErrorCategory::Unexpected => "unexpected",
            ErrorCategory::Unresponsive => "unresponsive",
        }
    }

    /// Parse back a persisted/label string, accepting pre-unification storage
    /// and metrics spellings (`throttle`, `too_many_requests`, `malformed`,
    /// `transport`, `cloudflare_access_denied`) so old rows still classify.
    #[must_use]
    pub fn from_str_opt(value: &str) -> Option<Self> {
        Some(match value {
            "captcha" => ErrorCategory::Captcha,
            "cloudflare_captcha" => ErrorCategory::CloudflareCaptcha,
            "recaptcha_captcha" => ErrorCategory::RecaptchaCaptcha,
            "access_denied" | "cloudflare_access_denied" => ErrorCategory::AccessDenied,
            "rate_limited" | "too_many_requests" | "throttle" => ErrorCategory::RateLimited,
            "timeout" => ErrorCategory::Timeout,
            "queue_expired" => ErrorCategory::QueueExpired,
            "parse" | "malformed" => ErrorCategory::Parse,
            "unexpected" | "transport" => ErrorCategory::Unexpected,
            "unresponsive" => ErrorCategory::Unresponsive,
            _ => return None,
        })
    }

    /// True for any of the three captcha/challenge variants.
    #[must_use]
    pub fn is_captcha(self) -> bool {
        matches!(
            self,
            ErrorCategory::Captcha
                | ErrorCategory::CloudflareCaptcha
                | ErrorCategory::RecaptchaCaptcha
        )
    }

    /// User-facing label for `unresponsive_engines` responses.
    #[must_use]
    pub fn user_label(self) -> &'static str {
        match self {
            ErrorCategory::Captcha
            | ErrorCategory::CloudflareCaptcha
            | ErrorCategory::RecaptchaCaptcha => "blocked by CAPTCHA",
            ErrorCategory::AccessDenied => "access denied",
            ErrorCategory::RateLimited => "rate limited",
            ErrorCategory::Parse => "bad upstream response",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::QueueExpired => "queue expired",
            ErrorCategory::Unexpected | ErrorCategory::Unresponsive => "error",
        }
    }
}

impl From<&EngineError> for ErrorCategory {
    fn from(err: &EngineError) -> Self {
        match err {
            EngineError::Captcha(_) => ErrorCategory::Captcha,
            EngineError::CloudflareCaptcha(_) => ErrorCategory::CloudflareCaptcha,
            EngineError::RecaptchaCaptcha(_) => ErrorCategory::RecaptchaCaptcha,
            EngineError::AccessDenied(_) | EngineError::CloudflareAccessDenied(_) => {
                ErrorCategory::AccessDenied
            }
            EngineError::TooManyRequests(_) => ErrorCategory::RateLimited,
            EngineError::Timeout => ErrorCategory::Timeout,
            EngineError::QueueExpired => ErrorCategory::QueueExpired,
            EngineError::Parse(_) => ErrorCategory::Parse,
            EngineError::Unexpected(_) => ErrorCategory::Unexpected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_as_str() {
        for cat in [
            ErrorCategory::Captcha,
            ErrorCategory::CloudflareCaptcha,
            ErrorCategory::RecaptchaCaptcha,
            ErrorCategory::AccessDenied,
            ErrorCategory::RateLimited,
            ErrorCategory::Timeout,
            ErrorCategory::QueueExpired,
            ErrorCategory::Parse,
            ErrorCategory::Unexpected,
            ErrorCategory::Unresponsive,
        ] {
            assert_eq!(ErrorCategory::from_str_opt(cat.as_str()), Some(cat));
        }
    }

    #[test]
    fn legacy_storage_labels_map_onto_new_vocabulary() {
        assert_eq!(
            ErrorCategory::from_str_opt("throttle"),
            Some(ErrorCategory::RateLimited)
        );
        assert_eq!(
            ErrorCategory::from_str_opt("too_many_requests"),
            Some(ErrorCategory::RateLimited)
        );
        assert_eq!(
            ErrorCategory::from_str_opt("malformed"),
            Some(ErrorCategory::Parse)
        );
        assert_eq!(
            ErrorCategory::from_str_opt("transport"),
            Some(ErrorCategory::Unexpected)
        );
        assert_eq!(
            ErrorCategory::from_str_opt("cloudflare_access_denied"),
            Some(ErrorCategory::AccessDenied)
        );
    }

    #[test]
    fn from_engine_error_collapses_captcha_variants_distinctly() {
        assert_eq!(
            ErrorCategory::from(&EngineError::Captcha("x".into())),
            ErrorCategory::Captcha
        );
        assert!(ErrorCategory::from(&EngineError::CloudflareCaptcha("x".into())).is_captcha());
    }
}
