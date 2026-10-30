#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn transport_cleanup_keeps_value_escapes_and_literal_delimiters() {
    let input = "\u{feff} \\(\\;GM[1]C[保留\\]括号(;)[x\\\\y]\r\n\\;B[pd]\\) \r\n";
    let output = normalize_sgf(input).unwrap();
    assert!(output.contains(r"C[保留\]括号(;)[x\\y]"));
    assert!(output.ends_with(";B[pd])"));
    assert!(!output.contains('\u{feff}'));
    assert_eq!(normalize_sgf(&output).unwrap(), output);
}

#[test]
fn first_variation_with_moves_is_selected_including_nested_variations() {
    let input = "(;KM[375]AP[GNU Go:3.8]AP[foxwq]C[root](;C[no moves](;C[still no]))(;W[dd]C[main](;B[pq]C[deep])(;B[qq]))(;B[aa]))";
    assert_eq!(
        normalize_sgf(input).unwrap(),
        "(;GM[1]FF[4]CA[UTF-8]AP[foxwq]SZ[19]KM[375]C[root];W[dd]C[main];B[pq]C[deep])"
    );
}

#[test]
fn original_crlf_escaped_values_comments_and_unknown_properties_are_retained() {
    let value = "line1\r\nline2\\\r\nline3\\]literal \\\\ end";
    let unknown = r"raw(;)[inside\]tail";
    let input = format!("(;KM[375]AP[foxwq]C[{value}]XX[{unknown}];B[aa]C[{value}]YY[{unknown}])");
    let output = normalize_sgf(&input).unwrap();
    assert!(output.contains(&format!("C[{value}]XX[{unknown}]")));
    assert!(output.contains(&format!(";B[aa]C[{value}]YY[{unknown}]")));
    assert!(output.contains("KM[375]"));
    assert_eq!(normalize_sgf(&output).unwrap(), output);
}

#[test]
fn bom_removal_matches_java_even_inside_values_without_removing_other_backslashes() {
    let input = "\u{feff}(;C[a\u{feff}b\\]c]XX[x\\\\y];\\B[aa])\u{feff}";
    let output = normalize_sgf(input).unwrap();
    assert!(output.contains(r"C[ab\]c]XX[x\\y]"));
    assert!(output.ends_with(";B[aa])"));
}

#[test]
fn leading_setup_properties_move_to_root_but_comments_remain() {
    let output = normalize_sgf("(;KM[0]HA[2];AB[pd][dp]C[setup note];W[pp];B[dd])").unwrap();
    assert!(output.contains("KM[0]HA[2]AB[pd][dp]"));
    assert!(output.ends_with(";C[setup note];W[pp];B[dd])"));
    assert!(!output.contains(";AB"));
    assert_eq!(normalize_sgf(&output).unwrap(), output);
}

#[test]
fn leading_black_setup_keeps_komi_in_original_fox_units() {
    let output = normalize_sgf(
        "(;KM[375]HA[0]AP[GNU Go:3.8]AP[foxwq];B[pd];B[pq]C[second setup];B[dd];B[dp];W[qo];B[qp])",
    )
    .unwrap();
    assert!(output.contains("AP[foxwq]"));
    assert!(!output.contains("GNU Go"));
    assert!(output.contains("KM[375]HA[4]AB[pd][pq][dd][dp]"));
    assert!(output.ends_with(";C[second setup];W[qo];B[qp])"));
    assert_eq!(normalize_sgf(&output).unwrap(), output);
}

#[test]
fn root_prefix_and_multi_value_moves_follow_java_color_order_without_losing_comments() {
    let output =
        normalize_sgf("(;KM[750]B[aa]W[bb]B[cc];W[dd][ee]B[ff]C[retained];W[gg])").unwrap();
    assert!(output.ends_with(";B[aa];W[bb];B[cc];W[dd]C[retained];B[ff];W[gg])"));
    assert!(!output.contains("[ee]"));
    assert!(output.contains("KM[750]"));
    let compatible = normalize_sgf("(;B[aa]W[bb];W[cc];B[dd])").unwrap();
    assert!(compatible.ends_with(";B[aa];W[cc];B[dd])"));
}

#[test]
fn root_properties_choose_last_nonempty_value_and_keep_raw_escapes() {
    let output =
        normalize_sgf(r"(;PB[first]PB[new\]name]PB[]AP[GNU Go:3.8]AP[foxwq]KM[375];W[aa])")
            .unwrap();
    assert!(output.contains(r"PB[new\]name]"));
    assert!(!output.contains("PB[first]"));
    assert!(!output.contains("PB[]"));
    assert!(output.contains("AP[foxwq]"));
    assert!(output.contains("KM[375]"));
}

#[test]
fn many_standard_variations_are_not_mistaken_for_fragmented_window_protocol() {
    let mut input = "(;B[pd](;C[comment-only])(;W[dd];B[qp])".to_owned();
    for _ in 0..64 {
        input.push_str("(;W[dp];B[pp])");
    }
    input.push(')');
    let output = normalize_sgf(&input).unwrap();
    assert!(output.ends_with(";B[pd];W[dd];B[qp])"));
}

#[test]
fn malformed_sgf_is_typed_failure_instead_of_partial_mainline_success() {
    for input in [
        "",
        " \u{feff}",
        "<html>server busy</html>",
        "()",
        "((;))",
        "(;C[unclosed)",
        "(;C[dangling\\",
        "(;C[value]) trailing",
        "(;B[aa])(;W[bb])",
        "(;B[aa](;W[bb]);B[cc])",
        "(;C[good]9[bad])",
        "(;ABC)",
        "(;[value])",
        "(;B[aa]) )",
        r"(;C[a\])",
    ] {
        assert!(
            matches!(
                normalize_sgf(input),
                Err(FoxError::InvalidResponse("chess"))
            ),
            "{input:?}"
        );
    }
}

#[test]
fn nesting_uses_bounded_arena_instead_of_recursive_calls() {
    let at_limit = "(;C[x]".repeat(MAX_DEPTH) + &")".repeat(MAX_DEPTH);
    assert!(normalize_sgf(&at_limit).is_ok());
    let too_deep = "(;C[x]".repeat(MAX_DEPTH + 1) + &")".repeat(MAX_DEPTH + 1);
    assert!(matches!(
        normalize_sgf(&too_deep),
        Err(FoxError::InvalidResponse("chess"))
    ));
}

#[test]
fn real_recorded_h5_game_keeps_177_moves_and_raw_komi_with_fox_origin() {
    let input = include_str!("../../../../fixtures/sgf/fox-recorded-2026-10-30.sgf");
    let output = normalize_sgf(input).unwrap();
    let tree = Parser::new(&output).parse().unwrap();
    let root = &tree[0].nodes[0];
    assert_eq!(
        root.iter().find(|p| p.name == "KM").unwrap().values,
        ["375"]
    );
    assert_eq!(root.iter().filter(|p| p.name == "AP").count(), 1);
    assert_eq!(
        root.iter().find(|p| p.name == "AP").unwrap().values,
        ["foxwq"]
    );
    assert_eq!(
        tree[0].nodes.iter().flat_map(|node| moves(node, 0)).count(),
        177
    );
    assert_eq!(normalize_sgf(&output).unwrap(), output);
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn generated_comments_retain_every_escape_and_branch_delimiter(
        chars in prop::collection::vec(prop::sample::select(vec!['a', '中', ']', '\\', '(', ')', ';', '[', '\r', '\n', ' ']), 0..150)
    ) {
        let mut value = String::new();
        for c in chars {
            if matches!(c, ']' | '\\') { value.push('\\'); }
            value.push(c);
        }
        let input = format!("(;C[{value}](;W[aa]XX[{value}])(;B[bb]))");
        let expected = format!("(;GM[1]FF[4]CA[UTF-8]SZ[19]C[{value}];W[aa]XX[{value}])");
        let output = normalize_sgf(&input).unwrap();
        prop_assert_eq!(&output, &expected);
        prop_assert_eq!(normalize_sgf(&output).unwrap(), output);
    }

    #[test]
    fn generated_deep_first_moved_variations_are_flattened_without_recursion(depth in 1usize..100) {
        let mut input = String::new();
        let mut expected = "(;GM[1]FF[4]CA[UTF-8]SZ[19]".to_owned();
        for index in 0..depth {
            let color = if index % 2 == 0 { 'W' } else { 'B' };
            input.push_str(&format!("(;{color}[aa]C[{index}]"));
            expected.push_str(&format!(";{color}[aa]C[{index}]"));
        }
        for _ in 0..depth { input.push(')'); }
        // Root comments stay at the root, whereas its move becomes a move node.
        expected = expected.replacen("SZ[19];W[aa]C[0]", "SZ[19]C[0];W[aa]", 1);
        expected.push(')');
        prop_assert_eq!(normalize_sgf(&input).unwrap(), expected);
    }
}
