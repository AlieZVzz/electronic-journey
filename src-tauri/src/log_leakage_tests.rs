use std::{fs, path::Path};

const LOG_MACROS: &[&str] = &[
    "tracing::trace!",
    "tracing::debug!",
    "tracing::info!",
    "tracing::warn!",
    "tracing::error!",
    "log::trace!",
    "log::debug!",
    "log::info!",
    "log::warn!",
    "log::error!",
    "print!",
    "println!",
    "eprint!",
    "eprintln!",
    "dbg!",
];

fn rust_sources(root: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).expect("Rust source directory must be readable") {
        let path = entry.expect("source entry must be readable").path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path
                .file_name()
                .is_none_or(|name| name != "log_leakage_tests.rs")
        {
            files.push(path);
        }
    }
}

fn invocation_at(source: &str, start: usize) -> String {
    let bytes = source.as_bytes();
    let open = source[start..]
        .find('(')
        .map(|offset| start + offset)
        .expect("log macro must have an argument list");
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for index in open..bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return source[start..=index]
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            }
            _ => {}
        }
    }
    panic!("unterminated log macro invocation");
}

#[test]
fn log_outputs_are_allowlisted_and_cannot_receive_sensitive_upload_values() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&source_root, &mut files);

    let mut actual = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("Rust source must be valid UTF-8");
        for macro_name in LOG_MACROS {
            for (start, _) in source.match_indices(macro_name) {
                if start > 0 && source.as_bytes()[start - 1].is_ascii_alphanumeric() {
                    continue;
                }
                actual.push(invocation_at(&source, start));
            }
        }
    }
    actual.sort();

    let sensitive_value_names = [
        "private_key_path",
        "private_key_passphrase",
        "passphrase",
        "key_path",
        "image_bytes",
        "content_sha256",
        "pixel_sha256",
        "local_path",
        "thumbnail_path",
        "remote_path",
        "remote_root",
        "server_response",
    ];
    for invocation in &actual {
        for sensitive in sensitive_value_names {
            assert!(
                !invocation.contains(sensitive),
                "log invocation references sensitive value `{sensitive}`: {invocation}"
            );
        }
    }

    let mut expected = vec![
        "eprintln!(\"startup {}: {} ms\", stage.as_str(), elapsed)".to_string(),
        concat!(
            "tracing::warn!( error_code = \"thumbnail_write_failed\", ",
            "capture_id = %capture_id, \"thumbnail could not be persisted\" )"
        )
        .to_string(),
    ];
    expected.sort();

    assert_eq!(
        actual, expected,
        "logging changed: security-review every log call before updating this allowlist; \
         private keys, passphrases, image bytes or hashes, local image paths, full remote paths, \
         and server response bodies must never be logged"
    );
}

#[test]
fn frontend_has_no_browser_console_log_sink() {
    let frontend_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
    let mut files = Vec::new();
    fn frontend_sources(root: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(root).expect("frontend source directory must be readable") {
            let path = entry
                .expect("frontend source entry must be readable")
                .path();
            if path.is_dir() {
                frontend_sources(&path, files);
            } else if path.extension().is_some_and(|extension| {
                matches!(extension.to_str(), Some("ts" | "tsx" | "js" | "jsx"))
            }) {
                files.push(path);
            }
        }
    }
    frontend_sources(&frontend_root, &mut files);

    let console_sinks = [
        "console.log(",
        "console.debug(",
        "console.info(",
        "console.warn(",
        "console.error(",
    ];
    let offenders = files
        .into_iter()
        .filter(|path| {
            let source = fs::read_to_string(path).expect("frontend source must be valid UTF-8");
            console_sinks.iter().any(|sink| source.contains(sink))
        })
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "browser console output requires a security review because frontend state may contain \
         private-key paths, passphrase inputs, image data, or full remote paths: {offenders:?}"
    );
}

#[test]
fn startup_trace_stage_is_a_closed_non_sensitive_value() {
    let stages = [
        super::StartupStage::ProcessEntered,
        super::StartupStage::SetupEntered,
        super::StartupStage::DatabaseReady,
        super::StartupStage::PageLoaded,
        super::StartupStage::BackgroundRecoveryFinished,
        super::StartupStage::CachedSnapshotRequested,
        super::StartupStage::PermissionRefreshStarted,
        super::StartupStage::PermissionRefreshFinished,
    ];

    assert_eq!(
        stages.map(super::StartupStage::as_str),
        [
            "process entered",
            "setup entered",
            "database ready",
            "page loaded",
            "background recovery finished",
            "cached snapshot requested",
            "permission refresh started",
            "permission refresh finished",
        ]
    );
}
