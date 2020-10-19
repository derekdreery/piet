use super::*;
use piet::TextLayout;

macro_rules! assert_close {
    ($val:expr, $target:expr, $tolerance:expr) => {{
        let min = $target - $tolerance;
        let max = $target + $tolerance;
        if $val < min || $val > max {
            panic!(
                "value {} outside target {} with tolerance {}",
                $val, $target, $tolerance
            );
        }
    }};

    ($val:expr, $target:expr, $tolerance:expr,) => {{
        assert_close!($val, $target, $tolerance)
    }};
}

#[test]
#[allow(clippy::float_cmp)]
fn hit_test_empty_string() {
    let layout = CairoText::new().new_text_layout("").build().unwrap();
    let pt = layout.hit_test_point(Point::new(0.0, 0.0));
    assert_eq!(pt.idx, 0);
    let pos = layout.hit_test_text_position(0);
    assert_eq!(pos.point.x, 0.0);
    assert_close!(pos.point.y, 10.0, 3.0);
    let line = layout.line_metric(0).unwrap();
    assert_close!(line.height, 12.0, 3.0);
}

#[test]
fn test_hit_test_text_position_basic() {
    let mut text_layout = CairoText::new();

    let input = "piet text!";

    let layout = text_layout.new_text_layout(&input[0..4]).build().unwrap();
    let piet_width = layout.size().width;

    let layout = text_layout.new_text_layout(&input[0..3]).build().unwrap();
    let pie_width = layout.size().width;

    let layout = text_layout.new_text_layout(&input[0..2]).build().unwrap();
    let pi_width = layout.size().width;

    let layout = text_layout.new_text_layout(&input[0..1]).build().unwrap();
    let p_width = layout.size().width;

    let layout = text_layout.new_text_layout("").build().unwrap();
    let null_width = layout.size().width;

    let full_layout = text_layout.new_text_layout(input).build().unwrap();
    let full_width = full_layout.size().width;

    assert_close!(
        full_layout.hit_test_text_position(4).point.x,
        piet_width,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(3).point.x,
        pie_width,
        3.0,
    );
    assert_close!(full_layout.hit_test_text_position(2).point.x, pi_width, 3.0,);
    assert_close!(full_layout.hit_test_text_position(1).point.x, p_width, 3.0,);
    assert_close!(
        full_layout.hit_test_text_position(0).point.x,
        null_width,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(10).point.x,
        full_width,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(11).point.x,
        full_width,
        3.0,
    );
}

#[test]
fn test_hit_test_text_position_complex_0() {
    let input = "é";
    assert_eq!(input.len(), 2);

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();

    assert_close!(layout.hit_test_text_position(0).point.x, 0.0, 3.0);
    assert_close!(
        layout.hit_test_text_position(2).point.x,
        layout.size().width,
        3.0,
    );

    // unicode segmentation is wrong on this one for now.
    //let input = "🤦\u{1f3fc}\u{200d}\u{2642}\u{fe0f}";

    //let mut text_layout = D2DText::new();
    //let font = text_layout.new_font_by_name("sans-serif", 12.0).build().unwrap();
    //let layout = text_layout.new_text_layout(&font, input, std::f64::INFINITY).build().unwrap();

    //assert_eq!(input.graphemes(true).count(), 1);
    //assert_eq!(layout.hit_test_text_position(0, true).map(|p| p.point_x as f64), Some(layout.size().width));
    //assert_eq!(input.len(), 17);

    let input = "\u{0023}\u{FE0F}\u{20E3}"; // #️⃣
    assert_eq!(input.len(), 7);
    assert_eq!(input.chars().count(), 3);

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();

    assert_close!(layout.hit_test_text_position(0).point.x, 0.0, 3.0);
    assert_close!(
        layout.hit_test_text_position(7).point.x,
        layout.size().width,
        3.0,
    );
}

#[test]
fn test_hit_test_text_position_complex_1() {
    // Notes on this input:
    // 6 code points
    // 7 utf-16 code units (1/1/1/1/1/2)
    // 14 utf-8 code units (2/1/3/3/1/4)
    // 4 graphemes
    let input = "é\u{0023}\u{FE0F}\u{20E3}1\u{1D407}"; // #️⃣,, 𝐇
    assert_eq!(input.len(), 14);

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();

    let test_layout_0 = text_layout.new_text_layout(&input[0..2]).build().unwrap();
    let test_layout_1 = text_layout.new_text_layout(&input[0..9]).build().unwrap();
    let test_layout_2 = text_layout.new_text_layout(&input[0..10]).build().unwrap();

    // Note: text position is in terms of utf8 code units
    assert_close!(layout.hit_test_text_position(0).point.x, 0.0, 3.0);
    assert_close!(
        layout.hit_test_text_position(2).point.x,
        test_layout_0.size().width,
        3.0,
    );
    assert_close!(
        layout.hit_test_text_position(9).point.x,
        test_layout_1.size().width,
        3.0,
    );
    assert_close!(
        layout.hit_test_text_position(10).point.x,
        test_layout_2.size().width,
        3.0,
    );
    assert_close!(
        layout.hit_test_text_position(14).point.x,
        layout.size().width,
        3.0,
    );

    // Code point boundaries, but not grapheme boundaries.
    // Width should stay at the current grapheme boundary.
    assert_close!(
        layout.hit_test_text_position(3).point.x,
        test_layout_0.size().width,
        3.0,
    );
    assert_close!(
        layout.hit_test_text_position(6).point.x,
        test_layout_0.size().width,
        3.0,
    );
}

#[test]
#[cfg(target_os = "linux")]
fn test_hit_test_point_basic_0() {
    let mut text_layout = CairoText::new();

    let layout = text_layout.new_text_layout("piet text!").build().unwrap();
    println!("text pos 4: {:?}", layout.hit_test_text_position(4)); // 23.0
    println!("text pos 5: {:?}", layout.hit_test_text_position(5)); // 27.0

    // test hit test point
    // all inside
    let pt = layout.hit_test_point(Point::new(22.5, 0.0));
    assert_eq!(pt.idx, 4);
    let pt = layout.hit_test_point(Point::new(23.0, 0.0));
    assert_eq!(pt.idx, 4);
    let pt = layout.hit_test_point(Point::new(25.0, 0.0));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(26.0, 0.0));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(27.0, 0.0));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(28.0, 0.0));
    assert_eq!(pt.idx, 5);

    // outside
    println!("layout_width: {:?}", layout.size().width); // 56.0

    let pt = layout.hit_test_point(Point::new(56.0, 0.0));
    assert_eq!(pt.idx, 10); // last text position
    assert_eq!(pt.is_inside, true);

    let pt = layout.hit_test_point(Point::new(57.0, 0.0));
    assert_eq!(pt.idx, 10); // last text position
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(-1.0, 0.0));
    assert_eq!(pt.idx, 0); // first text position
    assert_eq!(pt.is_inside, false);
}

#[test]
#[cfg(target_os = "macos")]
fn test_hit_test_point_basic_0() {
    let mut text_layout = CairoText::new();

    let layout = text_layout.new_text_layout("piet text!").build().unwrap();
    println!("text pos 4: {:?}", layout.hit_test_text_position(4)); // 19.34765625
    println!("text pos 5: {:?}", layout.hit_test_text_position(5)); // 22.681640625

    // test hit test point
    // all inside
    let pt = layout.hit_test_point(Point::new(19.0, 0.0));
    assert_eq!(pt.idx, 4);
    let pt = layout.hit_test_point(Point::new(20.0, 0.0));
    assert_eq!(pt.idx, 4);
    let pt = layout.hit_test_point(Point::new(21.0, 0.0));
    assert_eq!(pt.idx, 4);
    let pt = layout.hit_test_point(Point::new(22.0, 0.0));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(23.0, 0.0));
    assert_eq!(pt.idx, 5);

    // outside
    println!("layout_width: {:?}", layout.size().width); //45.357421875

    let pt = layout.hit_test_point(Point::new(45.0, 0.0));
    assert_eq!(pt.idx, 10); // last text position
    assert_eq!(pt.is_inside, true);

    let pt = layout.hit_test_point(Point::new(46.0, 0.0));
    assert_eq!(pt.idx, 10); // last text position
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(-1.0, 0.0));
    assert_eq!(pt.idx, 0); // first text position
    assert_eq!(pt.is_inside, false);
}

#[test]
#[cfg(target_os = "linux")]
// for testing that 'middle' assignment in binary search is correct
fn test_hit_test_point_basic_1() {
    let mut text_layout = CairoText::new();

    // base condition, one grapheme
    let layout = text_layout.new_text_layout("t").build().unwrap();
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0

    // two graphemes (to check that middle moves)
    let pt = layout.hit_test_point(Point::new(1.0, 0.0));
    assert_eq!(pt.idx, 0);

    let layout = text_layout.new_text_layout("te").build().unwrap();
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0
    println!("text pos 2: {:?}", layout.hit_test_text_position(2)); // 12.0

    let pt = layout.hit_test_point(Point::new(1.0, 0.0));
    assert_eq!(pt.idx, 0);
    let pt = layout.hit_test_point(Point::new(4.0, 0.0));
    assert_eq!(pt.idx, 1);
    let pt = layout.hit_test_point(Point::new(6.0, 0.0));
    assert_eq!(pt.idx, 1);
    let pt = layout.hit_test_point(Point::new(11.0, 0.0));
    assert_eq!(pt.idx, 2);
}

#[test]
#[cfg(target_os = "macos")]
// for testing that 'middle' assignment in binary search is correct
fn test_hit_test_point_basic_1() {
    let mut text_layout = CairoText::new();

    // base condition, one grapheme
    let layout = text_layout.new_text_layout("t").build().unwrap();
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0

    // two graphemes (to check that middle moves)
    let pt = layout.hit_test_point(Point::new(1.0, 0.0));
    assert_eq!(pt.idx, 0);

    let layout = text_layout.new_text_layout("te").build().unwrap();
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0
    println!("text pos 2: {:?}", layout.hit_test_text_position(2)); // 12.0

    let pt = layout.hit_test_point(Point::new(1.0, 0.0));
    assert_eq!(pt.idx, 0);
    let pt = layout.hit_test_point(Point::new(4.0, 0.0));
    assert_eq!(pt.idx, 1);
    let pt = layout.hit_test_point(Point::new(6.0, 0.0));
    assert_eq!(pt.idx, 1);
    let pt = layout.hit_test_point(Point::new(11.0, 0.0));
    assert_eq!(pt.idx, 2);
}

#[test]
#[cfg(target_os = "linux")]
fn test_hit_test_point_complex_0() {
    // Notes on this input:
    // 6 code points
    // 7 utf-16 code units (1/1/1/1/1/2)
    // 14 utf-8 code units (2/1/3/3/1/4)
    // 4 graphemes
    let input = "é\u{0023}\u{FE0F}\u{20E3}1\u{1D407}"; // #️⃣,, 𝐇

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();
    //println!("text pos 2: {:?}", layout.hit_test_text_position(2)); // 6.99999999
    //println!("text pos 9: {:?}", layout.hit_test_text_position(9)); // 24.0
    //println!("text pos 10: {:?}", layout.hit_test_text_position(10)); // 32.0
    //println!("text pos 14: {:?}", layout.hit_test_text_position(14)); // 39.0, line width

    let pt = layout.hit_test_point(Point::new(2.0, 0.0));
    assert_eq!(pt.idx, 0);
    let pt = layout.hit_test_point(Point::new(4.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(7.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(10.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(14.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(18.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(23.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(26.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(29.0, 0.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(32.0, 0.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(35.5, 0.0));
    assert_eq!(pt.idx, 14);
    let pt = layout.hit_test_point(Point::new(38.0, 0.0));
    assert_eq!(pt.idx, 14);
    let pt = layout.hit_test_point(Point::new(40.0, 0.0));
    assert_eq!(pt.idx, 14);
}

#[test]
#[cfg(target_os = "macos")]
fn test_hit_test_point_complex_0() {
    // Notes on this input:
    // 6 code points
    // 7 utf-16 code units (1/1/1/1/1/2)
    // 14 utf-8 code units (2/1/3/3/1/4)
    // 4 graphemes
    let input = "é\u{0023}\u{FE0F}\u{20E3}1\u{1D407}"; // #️⃣,, 𝐇

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();
    println!("text pos 2: {:?}", layout.hit_test_text_position(2)); // 6.673828125
    println!("text pos 9: {:?}", layout.hit_test_text_position(9)); // 28.55859375
    println!("text pos 10: {:?}", layout.hit_test_text_position(10)); // 35.232421875
    println!("text pos 14: {:?}", layout.hit_test_text_position(14)); // 42.8378905, line width

    let pt = layout.hit_test_point(Point::new(2.0, 0.0));
    assert_eq!(pt.idx, 0);
    let pt = layout.hit_test_point(Point::new(4.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(7.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(10.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(14.0, 0.0));
    assert_eq!(pt.idx, 2);
    let pt = layout.hit_test_point(Point::new(18.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(23.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(26.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(29.0, 0.0));
    assert_eq!(pt.idx, 9);
    let pt = layout.hit_test_point(Point::new(32.0, 0.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(35.5, 0.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(38.0, 0.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(40.0, 0.0));
    assert_eq!(pt.idx, 14);
    let pt = layout.hit_test_point(Point::new(43.0, 0.0));
    assert_eq!(pt.idx, 14);
}

#[test]
#[cfg(target_os = "linux")]
fn test_hit_test_point_complex_1() {
    // this input caused an infinite loop in the binary search when test position
    // > 21.0 && < 28.0
    //
    // This corresponds to the char 'y' in the input.
    let input = "tßßypi";

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();
    println!("text pos 0: {:?}", layout.hit_test_text_position(0)); // 0.0
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0
    println!("text pos 3: {:?}", layout.hit_test_text_position(3)); // 13.0
    println!("text pos 5: {:?}", layout.hit_test_text_position(5)); // 21.0
    println!("text pos 6: {:?}", layout.hit_test_text_position(6)); // 28.0
    println!("text pos 7: {:?}", layout.hit_test_text_position(7)); // 36.0
    println!("text pos 8: {:?}", layout.hit_test_text_position(8)); // 39.0, end

    let pt = layout.hit_test_point(Point::new(27.0, 0.0));
    assert_eq!(pt.idx, 6);
}

#[test]
#[cfg(target_os = "macos")]
fn test_hit_test_point_complex_1() {
    // this input caused an infinite loop in the binary search when test position
    // > 21.0 && < 28.0
    //
    // This corresponds to the char 'y' in the input.
    let input = "tßßypi";

    let mut text_layout = CairoText::new();
    let layout = text_layout.new_text_layout(input).build().unwrap();
    println!("text pos 0: {:?}", layout.hit_test_text_position(0)); // 0.0
    println!("text pos 1: {:?}", layout.hit_test_text_position(1)); // 5.0
    println!("text pos 3: {:?}", layout.hit_test_text_position(3)); // 13.0
    println!("text pos 5: {:?}", layout.hit_test_text_position(5)); // 21.0
    println!("text pos 6: {:?}", layout.hit_test_text_position(6)); // 28.0
    println!("text pos 7: {:?}", layout.hit_test_text_position(7)); // 36.0
    println!("text pos 8: {:?}", layout.hit_test_text_position(8)); // 39.0, end

    let pt = layout.hit_test_point(Point::new(27.0, 0.0));
    assert_eq!(pt.idx, 6);
}

#[test]
#[cfg(target_os = "linux")]
fn test_multiline_hit_test_text_position_basic() {
    let mut text_layout = CairoText::new();

    let input = "piet  text!";

    let layout = text_layout.new_text_layout(&input[0..3]).build().unwrap();
    let pie_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[0..4])
        .max_width(25.0)
        .build()
        .unwrap();
    let piet_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[0..5])
        .max_width(30.)
        .build()
        .unwrap();
    let piet_space_width = layout.size().width;

    // "text" should be on second line
    let layout = text_layout
        .new_text_layout(&input[6..10])
        .max_width(25.0)
        .build()
        .unwrap();
    let text_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..9])
        .max_width(25.0)
        .build()
        .unwrap();
    let tex_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..8])
        .max_width(25.0)
        .build()
        .unwrap();
    let te_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..7])
        .max_width(25.0)
        .build()
        .unwrap();
    let t_width = layout.size().width;

    let full_layout = text_layout
        .new_text_layout(input)
        .max_width(25.0)
        .build()
        .unwrap();

    println!("lm: {:#?}", full_layout.line_metrics);
    println!("layout width: {:#?}", full_layout.size().width);

    println!("'pie': {}", pie_width);
    println!("'piet': {}", piet_width);
    println!("'piet ': {}", piet_space_width);
    println!("'text': {}", text_width);
    println!("'tex': {}", tex_width);
    println!("'te': {}", te_width);
    println!("'t': {}", t_width);

    // NOTE these heights are representative of baseline-to-baseline measures
    let line_zero_baseline = full_layout
        .line_metric(0)
        .map(|l| l.y_offset + l.baseline)
        .unwrap();
    let line_one_baseline = full_layout
        .line_metric(1)
        .map(|l| l.y_offset + l.baseline)
        .unwrap();

    // these just test the x position of text positions on the second line
    assert_close!(
        full_layout.hit_test_text_position(10).point.x,
        text_width,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(9).point.x,
        tex_width,
        3.0,
    );
    assert_close!(full_layout.hit_test_text_position(8).point.x, te_width, 3.0,);
    assert_close!(full_layout.hit_test_text_position(7).point.x, t_width, 3.0,);
    // This should be beginning of second line
    assert_close!(full_layout.hit_test_text_position(6).point.x, 0.0, 3.0,);

    assert_close!(
        full_layout.hit_test_text_position(3).point.x,
        pie_width,
        3.0,
    );

    // This tests that trailing whitespace is included in the first line width.
    assert_close!(
        full_layout.hit_test_text_position(5).point.x,
        piet_space_width,
        3.0,
    );

    // These test y position of text positions on line 1 (0-index)
    assert_close!(
        full_layout.hit_test_text_position(10).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(9).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(8).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(7).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(6).point.y,
        line_one_baseline,
        3.0,
    );

    // this tests y position of 0 line
    assert_close!(
        full_layout.hit_test_text_position(5).point.y,
        line_zero_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(4).point.y,
        line_zero_baseline,
        3.0,
    );
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiline_hit_test_text_position_basic() {
    let mut text_layout = CairoText::new();

    let input = "piet  text!";
    let font = text_layout
        .font_family("Helvetica") // change this for osx
        .unwrap();

    let layout = text_layout
        .new_text_layout(&input[0..3])
        .font(font.clone(), 15.0)
        .max_width(30.0)
        .build()
        .unwrap();
    let pie_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[0..4])
        .font(font.clone(), 15.0)
        .max_width(25.0)
        .build()
        .unwrap();
    let piet_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[0..5])
        .font(font.clone(), 15.0)
        .max_width(30.0)
        .build()
        .unwrap();
    let piet_space_width = layout.size().width;

    // "text" should be on second line
    let layout = text_layout
        .new_text_layout(&input[6..10])
        .font(font.clone(), 15.0)
        .max_width(25.0)
        .build()
        .unwrap();
    let text_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..9])
        .font(font.clone(), 15.0)
        .max_width(25.0)
        .build()
        .unwrap();
    let tex_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..8])
        .font(font.clone(), 15.0)
        .max_width(25.0)
        .build()
        .unwrap();
    let te_width = layout.size().width;

    let layout = text_layout
        .new_text_layout(&input[6..7])
        .font(font.clone(), 15.0)
        .max_width(25.0)
        .build()
        .unwrap();
    let t_width = layout.size().width;

    let full_layout = text_layout
        .new_text_layout(input)
        .font(font, 15.0)
        .max_width(25.0)
        .build()
        .unwrap();

    println!("lm: {:#?}", full_layout.line_metrics);
    println!("layout width: {:#?}", full_layout.size().width);

    println!("'pie': {}", pie_width);
    println!("'piet': {}", piet_width);
    println!("'piet ': {}", piet_space_width);
    println!("'text': {}", text_width);
    println!("'tex': {}", tex_width);
    println!("'te': {}", te_width);
    println!("'t': {}", t_width);

    // NOTE these heights are representative of baseline-to-baseline measures
    let line_zero_baseline = full_layout
        .line_metric(0)
        .map(|l| l.y_offset + l.baseline)
        .unwrap();
    let line_one_baseline = full_layout
        .line_metric(1)
        .map(|l| l.y_offset + l.baseline)
        .unwrap();

    // these just test the x position of text positions on the second line
    assert_close!(
        full_layout.hit_test_text_position(10).point.x,
        text_width,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(9).point.x,
        tex_width,
        3.0,
    );
    assert_close!(full_layout.hit_test_text_position(8).point.x, te_width, 3.0,);
    assert_close!(full_layout.hit_test_text_position(7).point.x, t_width, 3.0,);
    // This should be beginning of second line
    assert_close!(full_layout.hit_test_text_position(6).point.x, 0.0, 3.0,);

    assert_close!(
        full_layout.hit_test_text_position(3).point.x,
        pie_width,
        3.0,
    );

    // This tests that trailing whitespace is included in the first line width.
    assert_close!(
        full_layout.hit_test_text_position(5).point.x,
        piet_space_width,
        3.0,
    );

    // These test y position of text positions on line 1 (0-index)
    assert_close!(
        full_layout.hit_test_text_position(10).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(9).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(8).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(7).point.y,
        line_one_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(6).point.y,
        line_one_baseline,
        3.0,
    );

    // this tests y position of 0 line
    assert_close!(
        full_layout.hit_test_text_position(5).point.y,
        line_zero_baseline,
        3.0,
    );
    assert_close!(
        full_layout.hit_test_text_position(4).point.y,
        line_zero_baseline,
        3.0,
    );
}

#[test]
#[cfg(target_os = "linux")]
// very basic testing that multiline works
fn test_multiline_hit_test_point_basic() {
    let input = "piet text most best";
    let mut text = CairoText::new();

    // this should break into four lines
    let layout = text.new_text_layout(input).max_width(30.0).build().unwrap();
    println!("text pos 01: {:?}", layout.hit_test_text_position(0)); // (0.0, 12.0)
    println!("text pos 06: {:?}", layout.hit_test_text_position(5)); // (0.0, 26.0)
    println!("text pos 11: {:?}", layout.hit_test_text_position(10)); // (0.0, 40.0)
    println!("text pos 16: {:?}", layout.hit_test_text_position(15)); // (0.0, 53.99999)

    let pt = layout.hit_test_point(Point::new(1.0, -1.0));
    assert_eq!(pt.idx, 0);
    assert_eq!(pt.is_inside, false);
    let pt = layout.hit_test_point(Point::new(1.0, 00.0));
    assert_eq!(pt.idx, 0);
    assert!(pt.is_inside);
    let pt = layout.hit_test_point(Point::new(1.0, 14.0));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(1.0, 28.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(1.0, 44.0));
    assert_eq!(pt.idx, 15);

    // over on y axis, but x still affects the text position
    let best_layout = text.new_text_layout("best").build().unwrap();
    println!("layout width: {:#?}", best_layout.size().width); // 26.0...

    let pt = layout.hit_test_point(Point::new(1.0, 56.0));
    assert_eq!(pt.idx, 15);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(25.0, 56.0));
    assert_eq!(pt.idx, 19);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(27.0, 56.0));
    assert_eq!(pt.idx, 19);
    assert_eq!(pt.is_inside, false);

    // under
    let piet_layout = text.new_text_layout("piet ").build().unwrap();
    println!("layout width: {:#?}", piet_layout.size().width); // 27.0...

    let pt = layout.hit_test_point(Point::new(1.0, -14.0)); // under
    assert_eq!(pt.idx, 0);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(26.0, -14.0)); // under
    assert_eq!(pt.idx, 5);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(28.0, -14.0)); // under
    assert_eq!(pt.idx, 5);
    assert_eq!(pt.is_inside, false);
}

#[test]
#[cfg(target_os = "macos")]
// very basic testing that multiline works
fn test_multiline_hit_test_point_basic() {
    let input = "piet text most best";
    let mut text = CairoText::new();

    let font = text.font_family("Helvetica").unwrap();
    // this should break into four lines
    let layout = text
        .new_text_layout(input)
        .font(font.clone(), 13.0)
        .max_width(30.0)
        .build()
        .unwrap();
    println!("text pos 01: {:?}", layout.hit_test_text_position(0)); // (0.0, 0.0)
    println!("text pos 06: {:?}", layout.hit_test_text_position(5)); // (0.0, 13.0)
    println!("text pos 11: {:?}", layout.hit_test_text_position(10)); // (0.0, 26.0)
    println!("text pos 16: {:?}", layout.hit_test_text_position(15)); // (0.0, 39.0)

    let pt = layout.hit_test_point(Point::new(1.0, -1.0));
    assert_eq!(pt.idx, 0);
    assert_eq!(pt.is_inside, false);
    let pt = layout.hit_test_point(Point::new(1.0, 00.0));
    assert_eq!(pt.idx, 0);
    assert!(pt.is_inside);
    let pt = layout.hit_test_point(Point::new(1.0, 12.));
    assert_eq!(pt.idx, 0);
    let pt = layout.hit_test_point(Point::new(1.0, 13.));
    assert_eq!(pt.idx, 5);
    let pt = layout.hit_test_point(Point::new(1.0, 26.0));
    assert_eq!(pt.idx, 10);
    let pt = layout.hit_test_point(Point::new(1.0, 39.0));
    assert_eq!(pt.idx, 15);
    assert!(pt.is_inside);

    // over on y axis, but x still affects the text position
    let best_layout = text
        .new_text_layout("best")
        .font(font.clone(), 13.0)
        .build()
        .unwrap();
    println!("layout width: {:#?}", best_layout.size().width); // 26.0...

    let pt = layout.hit_test_point(Point::new(1.0, 52.0));
    assert_eq!(pt.idx, 15);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(25.0, 52.0));
    assert_eq!(pt.idx, 19);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(27.0, 52.0));
    assert_eq!(pt.idx, 19);
    assert_eq!(pt.is_inside, false);

    // under
    let piet_layout = text
        .new_text_layout("piet ")
        .font(font, 13.0)
        .build()
        .unwrap();
    println!("layout width: {:#?}", piet_layout.size().width); // ???

    let pt = layout.hit_test_point(Point::new(1.0, -14.0)); // under
    assert_eq!(pt.idx, 0);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(25.0, -14.0)); // under
    assert_eq!(pt.idx, 5);
    assert_eq!(pt.is_inside, false);

    let pt = layout.hit_test_point(Point::new(27.0, -14.0)); // under
    assert_eq!(pt.idx, 5);
    assert_eq!(pt.is_inside, false);
}
