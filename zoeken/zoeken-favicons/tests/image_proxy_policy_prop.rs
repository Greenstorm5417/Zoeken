//! Property test for image proxy content policy.

use proptest::prelude::*;
use zoeken_favicons::{
    DEFAULT_MAX_IMAGE_BYTES, ImageProxyDecision, ImageProxyPolicy, ImageProxyRejection,
    image_proxy_decision,
};

fn ref_normalize(content_type: &str) -> String {
    let head = match content_type.find(';') {
        Some(idx) => &content_type[..idx],
        None => content_type,
    };
    head.trim().to_ascii_lowercase()
}

fn ref_content_type_allowed(content_type: &str, prefixes: &[String]) -> bool {
    let normalized = ref_normalize(content_type);
    if normalized.is_empty() {
        return false;
    }
    let mut allowed = false;
    for prefix in prefixes {
        let lower = prefix.to_ascii_lowercase();
        if normalized.starts_with(&lower) {
            allowed = true;
        }
    }
    allowed
}

fn ref_decision(
    content_type: Option<&str>,
    size: Option<u64>,
    prefixes: &[String],
    max_bytes: u64,
) -> ImageProxyDecision {
    if let Some(bytes) = size
        && bytes > max_bytes
    {
        return ImageProxyDecision::Reject(ImageProxyRejection::TooLarge);
    }
    match content_type {
        None => ImageProxyDecision::Reject(ImageProxyRejection::MissingContentType),
        Some(ct) => {
            if ref_content_type_allowed(ct, prefixes) {
                ImageProxyDecision::Serve
            } else {
                ImageProxyDecision::Reject(ImageProxyRejection::DisallowedContentType)
            }
        }
    }
}

fn base_content_type() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("image/png".to_string()),
        Just("image/jpeg".to_string()),
        Just("IMAGE/GIF".to_string()),
        Just("image/svg+xml".to_string()),
        Just("Image/WebP".to_string()),
        Just("binary/octet-stream".to_string()),
        Just("BINARY/OCTET-STREAM".to_string()),
        Just("application/octet-stream".to_string()),
        Just("text/html".to_string()),
        Just("application/json".to_string()),
        Just("video/mp4".to_string()),
        Just("text/plain".to_string()),
        Just("".to_string()),
        Just("   ".to_string()),
    ]
}

fn content_type() -> impl Strategy<Value = Option<String>> {
    let with_params = (
        base_content_type(),
        prop_oneof![
            Just(""),
            Just("; charset=binary"),
            Just(";charset=utf-8"),
            Just(" ; boundary=x"),
        ],
        prop::bool::ANY,
    )
        .prop_map(|(base, params, pad)| {
            let joined = format!("{base}{params}");
            if pad { format!("  {joined}  ") } else { joined }
        });

    prop_oneof![
        1 => Just(None),
        6 => with_params.prop_map(Some),
    ]
}

fn allowed_prefixes() -> impl Strategy<Value = Vec<String>> {
    prop_oneof![
        Just(vec![
            "image/".to_string(),
            "binary/octet-stream".to_string()
        ]),
        Just(vec!["image/".to_string()]),
        Just(vec!["binary/octet-stream".to_string()]),
        Just(vec!["image/png".to_string()]),
        Just(vec!["application/pdf".to_string()]),
        Just(vec![]),
    ]
}

fn max_bytes() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0u64),
        Just(1u64),
        Just(100u64),
        Just(1024u64),
        Just(DEFAULT_MAX_IMAGE_BYTES),
        1u64..(20 * 1024 * 1024),
    ]
}

#[derive(Debug, Clone)]
enum SizeSpec {
    None,
    AtLimit,
    BelowLimit,
    AboveLimit,
    Arbitrary(u64),
}

fn size_spec() -> impl Strategy<Value = SizeSpec> {
    prop_oneof![
        Just(SizeSpec::None),
        Just(SizeSpec::AtLimit),
        Just(SizeSpec::BelowLimit),
        Just(SizeSpec::AboveLimit),
        (0u64..(25 * 1024 * 1024)).prop_map(SizeSpec::Arbitrary),
    ]
}

fn resolve_size(spec: &SizeSpec, max: u64) -> Option<u64> {
    match spec {
        SizeSpec::None => None,
        SizeSpec::AtLimit => Some(max),
        SizeSpec::BelowLimit => Some(max.saturating_sub(1)),
        SizeSpec::AboveLimit => Some(max.saturating_add(1)),
        SizeSpec::Arbitrary(n) => Some(*n),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn image_proxy_content_policy_matches_reference(
        ct in content_type(),
        prefixes in allowed_prefixes(),
        max in max_bytes(),
        spec in size_spec(),
    ) {
        let size = resolve_size(&spec, max);
        let policy = ImageProxyPolicy::new(prefixes.clone(), max);

        let actual = image_proxy_decision(ct.as_deref(), size, &policy);
        let expected = ref_decision(ct.as_deref(), size, &prefixes, max);

        prop_assert_eq!(
            actual,
            expected,
            "content_type={:?} size={:?} prefixes={:?} max={}",
            ct, size, prefixes, max
        );

        let size_ok = size.map(|b| b <= max).unwrap_or(true);
        let ct_ok = ct
            .as_deref()
            .map(|c| ref_content_type_allowed(c, &prefixes))
            .unwrap_or(false);
        prop_assert_eq!(actual.is_serve(), size_ok && ct_ok);
    }
}
