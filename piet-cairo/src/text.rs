//! Text functionality for Piet cairo backend

//mod grapheme;
//mod lines;

use std::convert::TryInto;
use std::ops::RangeBounds;
use std::sync::Arc;

use cairo::{Matrix, ScaledFont};
use pango::{FontFamilyExt as _, FontMapExt as _};

use piet::kurbo::{Point, Rect, Size};
use piet::{
    util, Color, Error, FontFamily, FontStyle, HitTestPoint, HitTestPosition, LineMetric, Text,
    TextAlignment, TextAttribute, TextLayout, TextLayoutBuilder,
};

use unicode_segmentation::UnicodeSegmentation;

//use self::grapheme::{get_grapheme_boundaries, point_x_in_grapheme};

/// Right now, we don't need any state, as the "toy text API" treats the
/// access to system font information as a global. This will change.
// we use a phantom lifetime here to match the API of the d2d backend,
// and the likely API of something with access to system font information.
#[derive(Clone)]
pub struct PangoText {
    font_map: pango::FontMap,
    // Arc to allow non-borrow sharing with layout builder. We could use glib reference counting,
    // but I'm being rust-y.
    ctx: Arc<pango::Context>,
}

#[derive(Clone)]
pub struct PangoTextLayout {
    pub(crate) layout: pango::Layout,
    text: Arc<str>,
}

pub struct PangoTextLayoutBuilder {
    text: Arc<str>,
    defaults: util::LayoutDefaults,
    alignment: TextAlignment,
    width_constraint: f64,
    ctx: Arc<pango::Context>,
}

impl PangoText {
    /// Create a new factory that satisfies the piet `Text` trait.
    #[allow(clippy::new_without_default)]
    pub fn new() -> PangoText {
        let font_map = pangocairo::FontMap::get_default().expect("could not get default font map");
        let ctx = Arc::new(
            pango::FontMapExt::create_context(&font_map).expect("could not create pango context"),
        );
        PangoText { font_map, ctx }
    }
}

impl Text for PangoText {
    type TextLayout = PangoTextLayout;
    type TextLayoutBuilder = PangoTextLayoutBuilder;

    fn font_family(&mut self, family_name: &str) -> Option<FontFamily> {
        // Pango doesn't really care about font families - it decides whether a font exists or not
        // once it has all information, inc. weight, size, etc. Here, we just check that *a* font
        // exists with the given family (a.k.a. face).
        if self
            .font_map
            .list_families() // allocates
            .into_iter()
            .filter(|fam| {
                fam.get_name()
                    .map(|name| name == family_name)
                    .unwrap_or(false)
            })
            .next()
            .is_some()
        {
            Some(FontFamily::new_unchecked(family_name))
        } else {
            None
        }
    }

    fn load_font(&mut self, _data: &[u8]) -> Result<FontFamily, Error> {
        // For pango, you have to have the font in a file and tell fontconfig where it is, if you
        // want to load custom fonts.
        Err(Error::NotSupported)
    }

    fn new_text_layout(&mut self, text: impl Into<Arc<str>>) -> Self::TextLayoutBuilder {
        PangoTextLayoutBuilder {
            defaults: util::LayoutDefaults::default(),
            text: text.into(),
            width_constraint: f64::INFINITY,
            // TODO this choice is somewhat arbitary.
            alignment: TextAlignment::Start,
            ctx: self.ctx.clone(),
        }
    }
}

impl TextLayoutBuilder for PangoTextLayoutBuilder {
    type Out = PangoTextLayout;

    fn max_width(mut self, width: f64) -> Self {
        self.width_constraint = width;
        self
    }

    fn alignment(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    fn default_attribute(mut self, attribute: impl Into<TextAttribute>) -> Self {
        self.defaults.set(attribute);
        self
    }

    fn range_attribute(
        self,
        range: impl RangeBounds<usize>,
        attribute: impl Into<TextAttribute>,
    ) -> Self {
        self
    }

    fn build(self) -> Result<Self::Out, Error> {
        // build font description
        let mut desc = pango::FontDescription::new();
        desc.set_family(self.defaults.font.name());
        // TODO is this right - not set_size?
        desc.set_absolute_size(self.defaults.font_size);
        // TODO align
        // TODO weight
        desc.set_style(match self.defaults.style {
            FontStyle::Regular => pango::Style::Normal,
            FontStyle::Italic => pango::Style::Italic,
        });

        // build text layout
        let layout = pango::Layout::new(&self.ctx);
        layout.set_font_description(Some(&desc));
        // Justification is separate from align in pango.
        if let TextAlignment::Justified = self.alignment {
            layout.set_justify(true);
        }
        // in pango, RTL means `Left` and `Right` swap meanings.
        layout.set_alignment(match self.alignment {
            TextAlignment::Start | TextAlignment::Justified => pango::Alignment::Left,
            TextAlignment::Center => pango::Alignment::Center,
            TextAlignment::End => pango::Alignment::Right,
        });
        let mut layout = PangoTextLayout {
            layout,
            text: self.text,
        };

        layout.update_width(self.width_constraint)?;
        Ok(layout)
    }
}

impl PangoTextLayout {
    /// A helper method to create a run iterator for the current layout, and then move it to the
    /// line number given. If the line doesn't exist, `None` is returned.
    fn iter_at_line_number(&self, line_number: usize) -> Option<pango::LayoutIter> {
        let mut iter = self.layout.get_iter()?;
        for _ in 0..line_number {
            if iter.at_last_line() {
                return None;
            }
            iter.next_line();
        }
        Some(iter)
    }

    /// Get the text from the line that `iter` is currently on. Returns indexes into text string
    /// (start, end).
    fn iter_line_text(&self, iter: &mut pango::LayoutIter) -> (usize, usize) {
        let start = iter
            .get_index()
            .try_into()
            .expect("LayoutIter::get_index: i32 -> usize conv failed");
        let end = if iter.at_last_line() {
            self.text.len()
        } else {
            iter.next_line();
            iter.get_index()
                .try_into()
                .expect("LayoutIter::get_index: i32 -> usize conv failed")
        };
        (start, end)
    }
}

// In pango, "logical" bounds are what you use for positioning, "ink" bounds are where there will
// actually be drawing (e.g. for marking dirty areas).
impl TextLayout for PangoTextLayout {
    fn size(&self) -> Size {
        self.image_bounds().size()
    }

    fn image_bounds(&self) -> Rect {
        let r = self.layout.get_extents().1;
        let x0 = pango::units_to_double(r.x);
        let y0 = pango::units_to_double(r.y);
        let x1 = x0 + pango::units_to_double(r.width);
        let y1 = y0 + pango::units_to_double(r.height);
        Rect::new(x0, y0, x1, y1)
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn update_width(&mut self, new_width: impl Into<Option<f64>>) -> Result<(), Error> {
        // TODO no idea if this conversion is correct. I propose trial and error to find out.
        // -1 definitely means no bound.
        let new_width = match new_width.into() {
            None => -1,
            Some(x) if x == f64::INFINITY => -1,
            Some(x) => pango::units_from_double(x),
            // TODO should negative width be an error? In this impl it will probably mean the same
            // as `None`.
        };
        self.layout.set_width(new_width);
        Ok(())
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        // I can't figure out if this is possible using LayoutLine (I don't think it is), so I'm
        // using `LayoutIter`.
        let mut iter = self.iter_at_line_number(line_number)?;
        // We can now get the byte indexes for the line. This could be eagerly calculated or cached
        // if desired.
        let (start, end) = self.iter_line_text(&mut iter);
        Some(&self.text[start..end])
    }

    fn line_metric(&self, line_number: usize) -> Option<LineMetric> {
        let mut iter = self.iter_at_line_number(line_number)?;
        let (
            _ink,
            pango::Rectangle {
                x: _,
                y,
                width: _,
                height,
            },
        ) = iter.get_line_extents();
        let (start, end) = self.iter_line_text(&mut iter);
        let line_text = &self.text[start..end];
        Some(LineMetric {
            // Index into str - start of line
            start_offset: start,
            // Index into str - start of next line
            end_offset: end,
            // length of whitespace in code units (bytes for utf-8, which we are)
            trailing_whitespace: count_trailing_whitespace(line_text),
            // Distance from top of line to baseline. Usually positive.
            baseline: pango::units_to_double(iter.get_baseline()),
            // Distance from baseline to max descent.
            height: pango::units_to_double(height),
            // Distance of line from top of layout (y in layout-space)
            y_offset: pango::units_to_double(y),
        })
    }

    fn line_count(&self) -> usize {
        self.layout
            .get_line_count()
            .try_into()
            .expect("PangoTextLayout::line_count: i32 -> usize conv failed")
    }

    fn hit_test_point(&self, point: Point) -> HitTestPoint {
        // XXX is_inside is always false - pango hit testing does not test the glyph outline,
        // only the bounding box.
        // The first return value (bool) denotes whether the point is in text - this is *not* the
        // same as `is_inside`.
        // TODO again I don't know if `units_from_double` gives me what I want. I'm slowly
        // convincing myself that it should, but I'm not in any way sure.
        let x = pango::units_from_double(point.x);
        let y = pango::units_from_double(point.y);
        let (_, index, _) = self.layout.xy_to_index(x, y);
        let mut htp = HitTestPoint::default();
        htp.idx = index
            .try_into()
            .expect("PangoTextLayout::hit_text_point: i32 -> usize conv failed");
        htp
    }

    fn hit_test_text_position(&self, idx: usize) -> HitTestPosition {
        let idx: i32 = idx
            .try_into()
            .expect("PangoTextLayout::hit_test_text_position: usize -> i32 conv failed");
        let (line, x_pos) = self.layout.index_to_line_x(idx, false);
        let mut htp = HitTestPosition::default();
        htp.line = line
            .try_into()
            .expect("PangoTextLayout::hit_test_text_position: i32 -> usize conv failed");
        // TODO check conv.
        htp.point.x = pango::units_to_double(x_pos);
        // to get baseline y, we need to use the run iterator again.
        let mut iter = self
            .iter_at_line_number(htp.line)
            .expect("pango reported text is on a line, but it also reports the line doesn't exist");
        htp.point.y = pango::units_to_double(iter.get_baseline());
        htp
    }
}

// TODO: is non-breaking space trailing whitespace? Check with dwrite and
// coretext
fn count_trailing_whitespace(line: &str) -> usize {
    line.chars().rev().take_while(|c| c.is_whitespace()).count()
}

#[cfg(test)]
mod test {
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
        let layout = PangoText::new().new_text_layout("").build().unwrap();
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
        let mut text_layout = PangoText::new();

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

        let mut text_layout = PangoText::new();
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

        let mut text_layout = PangoText::new();
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

        let mut text_layout = PangoText::new();
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
        let mut text_layout = PangoText::new();

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
        let mut text_layout = PangoText::new();

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
        let mut text_layout = PangoText::new();

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
        let mut text_layout = PangoText::new();

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

        let mut text_layout = PangoText::new();
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

        let mut text_layout = PangoText::new();
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

        let mut text_layout = PangoText::new();
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

        let mut text_layout = PangoText::new();
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
        let mut text_layout = PangoText::new();

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

        //println!("lm: {:#?}", full_layout.line_metrics);
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
        let mut text_layout = PangoText::new();

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
        let mut text = PangoText::new();

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
        let mut text = PangoText::new();

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
}
