//! Tests for bundled data loader error reporting and language detection.
//! Separate from inline tests to exercise only the public loader API.

use std::path::{Path, PathBuf};

use zoeken_data::{DataError, detect_language, load_bundle};

/// The bundled data files `load_bundle` requires, each paired with minimal
/// content that parses successfully. Order mirrors the load order in
/// `load_bundle` so tests can reason about which file is reported first.
const REQUIRED_FILES: &[(&str, &str)] = &[
    ("external_bangs.json", r#"{"trie": {}}"#),
    ("currencies.json", r#"{"iso4217": {}, "names": {}}"#),
    ("wikidata_units.json", r#"{}"#),
    ("engine_traits.json", r#"{}"#),
    ("useragents.json", r#"{"os": [], "ua": "", "versions": []}"#),
    ("gsa_useragents.txt", "GSA/1.0\n"),
    ("locales.json", r#"{"LOCALE_NAMES": {}, "RTL_LOCALES": []}"#),
    (
        "doi_resolvers.json",
        r#"{"default":"doi.org","resolvers":{"doi.org":"https://doi.org/"}}"#,
    ),
    (
        "autocomplete_backends.json",
        r#"{"backends":["duckduckgo"]}"#,
    ),
    ("limiter.toml", "[botdetection]\nip_limit = false\n"),
    (
        "info_pages.json",
        r#"{"default_locale":"en","locales":{"en":{"about":{"title":"About","content":"About"}}}}"#,
    ),
];

/// A self-cleaning temporary directory (no external crate needed).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        // Uniqueness: process id + a monotonically increasing per-process counter
        // + nanosecond clock. Sufficient to isolate concurrent test cases.
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "zoeken-data-test-{tag}-{}-{n}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_valid_bundle(dir: &Path) {
    for (file, contents) in REQUIRED_FILES {
        std::fs::write(dir.join(file), contents).unwrap_or_else(|e| panic!("write {file}: {e}"));
    }
}

fn affected_file(err: &DataError) -> &str {
    match err {
        DataError::Read { file, .. } => file,
        DataError::Parse { file, .. } => file,
    }
}

#[test]
fn valid_minimal_bundle_loads_successfully() {
    // Establishes that the fixtures are genuinely valid, so the failure tests
    // below fail only because of the file they intentionally break.
    let dir = TempDir::new("valid");
    write_valid_bundle(dir.path());
    assert!(load_bundle(dir.path()).is_ok());
}

#[test]
fn empty_directory_reports_first_missing_file() {
    let dir = TempDir::new("empty");
    let err = load_bundle(dir.path()).expect_err("empty dir must abort loading");
    assert!(
        matches!(err, DataError::Read { .. }),
        "a missing file is a Read error, got: {err:?}"
    );
    assert_eq!(
        affected_file(&err),
        "external_bangs.json",
        "the first-loaded missing file should be identified"
    );
}

#[test]
fn each_missing_file_is_identified_by_name() {
    for (missing, _) in REQUIRED_FILES {
        let dir = TempDir::new("missing");
        write_valid_bundle(dir.path());
        std::fs::remove_file(dir.path().join(missing)).expect("remove target file");

        let err = load_bundle(dir.path())
            .err()
            .unwrap_or_else(|| panic!("missing {missing} must abort loading"));

        assert!(
            matches!(err, DataError::Read { .. }),
            "missing {missing} should be a Read error, got: {err:?}"
        );
        assert_eq!(
            affected_file(&err),
            *missing,
            "the error should identify the missing file"
        );
        // The Display message should also mention the affected file so startup
        // can surface it.
        assert!(
            err.to_string().contains(missing),
            "error message `{err}` should mention `{missing}`"
        );
    }
}

#[test]
fn malformed_json_file_is_identified_by_name() {
    let dir = TempDir::new("malformed");
    write_valid_bundle(dir.path());
    std::fs::write(dir.path().join("currencies.json"), "{ this is not json ]")
        .expect("overwrite currencies.json with garbage");

    let err = load_bundle(dir.path()).expect_err("malformed file must abort loading");
    assert!(
        matches!(err, DataError::Parse { .. }),
        "a malformed file is a Parse error, got: {err:?}"
    );
    assert_eq!(affected_file(&err), "currencies.json");
    assert!(
        err.to_string().contains("currencies.json"),
        "error message `{err}` should mention the affected file"
    );
}

#[test]
fn detects_expected_languages_for_known_examples() {
    // Each example is chosen to be unambiguous for the detector: a full,
    // natural sentence, several using a script unique to the language.
    let cases: &[(&str, &str)] = &[
        (
            "eng",
            "The quick brown fox jumps over the lazy dog while the sun is shining brightly today.",
        ),
        (
            "rus",
            "Быстрая коричневая лиса прыгает через ленивую собаку, пока ярко светит солнце.",
        ),
        (
            "jpn",
            "これは日本語で書かれた文章です。今日はとても良い天気ですね。",
        ),
        (
            "ell",
            "Αυτή είναι μια πρόταση γραμμένη στα ελληνικά για τον έλεγχο της ανίχνευσης γλώσσας.",
        ),
    ];

    for (expected, text) in cases {
        let detected = detect_language(text)
            .unwrap_or_else(|| panic!("a language should be detected for: {text}"));
        assert_eq!(
            detected.as_str(),
            *expected,
            "unexpected language for example text: {text}"
        );
    }
}

#[test]
fn empty_and_whitespace_text_detects_no_language() {
    assert!(detect_language("").is_none(), "empty text has no language");
    assert!(
        detect_language("   \n\t  ").is_none(),
        "whitespace-only text has no language"
    );
}
