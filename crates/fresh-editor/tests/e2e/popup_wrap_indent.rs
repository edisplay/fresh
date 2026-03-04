//! E2E test for popup line wrapping with hanging indent.
//!
//! When a line in a popup is too long and wraps, the continuation line(s)
//! should be indented to match the original line's leading whitespace.

use crate::common::harness::EditorTestHarness;
use fresh::model::event::{Event, PopupContentData, PopupData, PopupKindHint, PopupPositionData};

/// Verify that wrapped continuation lines in a popup preserve the hanging
/// indent of the original line.
///
/// Without the fix, continuation lines start at column 0 inside the popup,
/// making it impossible to tell which parameter a wrapped description belongs
/// to. With the fix, continuation lines are indented to match.
#[test]
fn test_popup_wrapped_lines_have_hanging_indent() -> anyhow::Result<()> {
    // Use a narrow terminal so the popup is narrow enough to force wrapping
    let mut harness = EditorTestHarness::new(60, 24)?;

    // Build content that mimics signature help parameter documentation.
    // The indented lines are long enough to wrap in a width-40 popup.
    let content = vec![
        "print(*values, sep, end, file, flush)".to_string(),
        "---".to_string(),
        "    sep  string inserted between values, default a space, used to join all output values together".to_string(),
        "    end  string appended after the last value, default a newline character sequence".to_string(),
    ];

    harness.apply_event(Event::ShowPopup {
        popup: PopupData {
            kind: PopupKindHint::Text,
            title: Some("Signature Help".to_string()),
            description: None,
            transient: false,
            content: PopupContentData::Text(content),
            position: PopupPositionData::Centered,
            width: 40, // Narrow enough to force wrapping of the indented lines
            max_height: 20,
            bordered: true,
        },
    })?;

    harness.render()?;

    let screen = harness.screen_to_string();
    eprintln!("[TEST] Screen:\n{}", screen);

    // Find all lines inside the popup that contain text.
    // The indented lines should wrap, and the continuation lines
    // must start with spaces (the hanging indent), not with content
    // at column 0 of the popup.
    //
    // Look for the "sep" line and its continuation: both should
    // start with leading spaces inside the popup.
    let popup_lines: Vec<&str> = screen
        .lines()
        .filter(|line| line.contains("sep") || line.contains("end"))
        .collect();

    eprintln!("[TEST] Popup lines with 'sep'/'end': {:?}", popup_lines);

    // Find the line starting with "sep" (the original indented line)
    let sep_line = screen
        .lines()
        .find(|line| line.contains("sep  string inserted"))
        .expect("Should find the 'sep' parameter line");

    // Find the continuation of the sep line — it should contain "used to join"
    // or "output values" (the wrapped portion)
    let continuation = screen
        .lines()
        .find(|line| {
            (line.contains("used to join") || line.contains("output values"))
                && !line.contains("sep  string")
        })
        .expect("Should find a wrapped continuation line for 'sep'");

    eprintln!("[TEST] sep_line: {:?}", sep_line);
    eprintln!("[TEST] continuation: {:?}", continuation);

    // The continuation line should have leading spaces (hanging indent).
    // Find where the text content starts in the continuation line by
    // looking for the first non-space, non-border character.
    // The border character '│' appears at the popup edges.
    let content_after_border = continuation
        .split('│')
        .nth(1)
        .expect("Continuation should be inside popup border");

    let leading_spaces = content_after_border
        .chars()
        .take_while(|ch| *ch == ' ')
        .count();

    eprintln!(
        "[TEST] Content after border: {:?}, leading_spaces: {}",
        content_after_border, leading_spaces
    );

    // The original line has 4 spaces of indent. The continuation must
    // also have at least 4 spaces of indent (hanging indent).
    assert!(
        leading_spaces >= 4,
        "Wrapped continuation line should have hanging indent (>= 4 spaces), \
         but only has {} leading spaces.\n\
         Continuation line: {:?}",
        leading_spaces,
        continuation,
    );

    Ok(())
}
