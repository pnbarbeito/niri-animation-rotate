use std::path::Path;

/// Strip block comments from a line, tracking `in_block` state across lines.
///
/// Returns the line content with `/* ... */` sections removed.
fn strip_block_comments(line: &str, in_block: &mut bool) -> String {
    let mut result = String::new();
    let mut rest = line;

    while !rest.is_empty() {
        if *in_block {
            // We're inside a block comment — look for the closing marker
            if let Some(end) = rest.find("*/") {
                rest = &rest[end + 2..];
                *in_block = false;
            } else {
                // Rest of line is inside the block comment
                break;
            }
        } else {
            // Not in a block comment — look for a start marker
            if let Some(start) = rest.find("/*") {
                result.push_str(&rest[..start]);
                rest = &rest[start + 2..];
                *in_block = true;
            } else {
                result.push_str(rest);
                break;
            }
        }
    }

    result
}

/// Parse a Niri animation KDL file to extract its total duration in milliseconds.
///
/// Scans the file line by line for `duration-ms <int>` values and takes the maximum.
/// Also looks for a top-level `slowdown <float>` multiplier (e.g., `slowdown 1.5`)
/// which is applied to the result.
///
/// Comments are handled:
///   - Lines starting with `//` are skipped
///   - Content between `/*` and `*/` is skipped (block comments)
///
/// Returns `None` if no `duration-ms` values are found in the file.
pub fn parse_animation_duration(path: &Path) -> Option<u64> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut durations: Vec<u64> = Vec::new();
    let mut slowdown: f64 = 1.0;
    let mut in_block_comment = false;

    for raw_line in content.lines() {
        let mut line = strip_block_comments(raw_line, &mut in_block_comment);

        // Strip inline // comments so that keywords inside them are ignored
        if let Some(pos) = line.find("//") {
            line.truncate(pos);
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Extract duration-ms values
        if let Some(pos) = line.find("duration-ms") {
            let after = &line[pos + "duration-ms".len()..];
            if let Some(val_str) = after.split_whitespace().next()
                && let Ok(val) = val_str.parse::<u64>()
            {
                durations.push(val);
            }
        }

        // Extract slowdown multiplier
        if let Some(pos) = line.find("slowdown ") {
            let after = &line[pos + "slowdown ".len()..];
            if let Some(val_str) = after.split_whitespace().next()
                && let Ok(val) = val_str.parse::<f64>()
            {
                slowdown = val;
            }
        }
    }

    let max_ms = durations.into_iter().max()?;
    Some((max_ms as f64 * slowdown).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_temp_file(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("niri-duration-parser-tests");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_single_duration() {
        let path = write_temp_file(
            "single.kdl",
            "animations {\n    window-open {\n        duration-ms 400\n    }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(400));
    }

    #[test]
    fn test_max_of_multiple_durations() {
        let path = write_temp_file(
            "multi.kdl",
            "animations {\n    window-open { duration-ms 300 }\n    window-close { duration-ms 500 }\n    workspace-switch { duration-ms 200 }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(500));
    }

    #[test]
    fn test_slowdown_multiplier() {
        let path = write_temp_file(
            "slowdown.kdl",
            "animations {\n    slowdown 1.5\n    window-open {\n        duration-ms 280\n    }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(420)); // 280 * 1.5
    }

    #[test]
    fn test_commented_slowdown_ignored() {
        // tv_crt.kdl has //slowdown 3.0 commented out, but also slowdown 1.0 active
        let path = write_temp_file(
            "commented.kdl",
            "animations {\n    slowdown 1.0\n    //slowdown 3.0\n    window-open { duration-ms 400 }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(400)); // slowdown 1.0, not 3.0
    }

    #[test]
    fn test_block_comment_ignored() {
        let path = write_temp_file(
            "block_comment.kdl",
            "animations {\n    /* duration-ms 999 */\n    window-open { duration-ms 300 }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(300));
    }

    #[test]
    fn test_no_duration_returns_none() {
        let path = write_temp_file(
            "no_duration.kdl",
            "animations {\n    window-open {\n        spring damping-ratio=0.9 stiffness=800\n    }\n}",
        );
        assert_eq!(parse_animation_duration(&path), None);
    }

    #[test]
    fn test_only_slowdown_no_duration_returns_none() {
        let path = write_temp_file(
            "only_slowdown.kdl",
            "animations {\n    slowdown 2.0\n}",
        );
        assert_eq!(parse_animation_duration(&path), None);
    }

    #[test]
    fn test_duration_on_same_line_as_node() {
        let path = write_temp_file(
            "same_line.kdl",
            "animations {\n    window-open { duration-ms 400\n        custom-shader r\"...\"\n    }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(400));
    }

    #[test]
    fn test_nonexistent_file_returns_none() {
        let path = PathBuf::from("/nonexistent/path/to/file.kdl");
        assert_eq!(parse_animation_duration(&path), None);
    }

    #[test]
    fn test_slowdown_below_one() {
        let path = write_temp_file(
            "fast.kdl",
            "animations {\n    slowdown 0.5\n    window-open { duration-ms 400 }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(200)); // 400 * 0.5
    }

    #[test]
    fn test_real_bloom_kdl() {
        // Simplified version of bloom.kdl structure
        let path = write_temp_file(
            "bloom.kdl",
            r#"animations {
    workspace-switch {
        spring damping-ratio=0.9 stiffness=800 epsilon=0.0001
    }
    window-open {
        duration-ms 200
        custom-shader r"..."
    }
    window-close {
        duration-ms 300
        custom-shader r"..."
    }
}"#,
        );
        assert_eq!(parse_animation_duration(&path), Some(300)); // max(200, 300)
    }

    #[test]
    fn test_real_tv_crt_kdl() {
        // Simplified version of tv_crt.kdl structure (slowdown + durations)
        let path = write_temp_file(
            "tv_crt.kdl",
            r#"animations {
    slowdown 1.0
    window-open {
        duration-ms 260
        custom-shader r"..."
    }
    window-close {
        duration-ms 180
        custom-shader r"..."
    }
    window-resize {
        duration-ms 160
    }
}"#,
        );
        assert_eq!(parse_animation_duration(&path), Some(260)); // max(260, 180, 160) * 1.0
    }

    #[test]
    fn test_real_prism_fold_kdl() {
        // Simplified prism_fold.kdl — slowdown 1.5, window-open 280ms
        let path = write_temp_file(
            "prism_fold.kdl",
            r#"animations {
    slowdown 1.5
    window-open {
        duration-ms 280
        custom-shader r"..."
    }
    window-close {
        duration-ms 180
        custom-shader r"..."
    }
}"#,
        );
        assert_eq!(parse_animation_duration(&path), Some(420)); // max(280, 180) * 1.5 = 420
    }

    #[test]
    fn test_inline_comment_slowdown_ignored() {
        // slowdown inside an inline // comment should be ignored
        let path = write_temp_file(
            "inline_comment.kdl",
            "animations {\n    slowdown 1.0\n    window-open { duration-ms 400 } // slowdown 3.0\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(400)); // 1.0, not 3.0
    }

    #[test]
    fn test_line_comment_before_slowdown() {
        // The slowdown line is preceded by a comment, but slowdown itself is on its own line
        let path = write_temp_file(
            "comment_before.kdl",
            "animations {\n    // Slow down all animations\n    slowdown 2.0\n    window-open { duration-ms 400 }\n}",
        );
        assert_eq!(parse_animation_duration(&path), Some(800));
    }
}
