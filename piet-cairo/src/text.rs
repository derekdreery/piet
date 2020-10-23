//! Text functionality for Piet cairo backend

mod grapheme;
mod lines;

use std::ops::RangeBounds;
use std::rc::Rc;
use std::sync::Arc;

use cairo::{FontFace, FontOptions, Matrix, ScaledFont, UserDataKey};

use piet::kurbo::{Point, Rect, Size};
use piet::{
    util, Color, Error, FontFamily, FontStyle, FontWeight, HitTestPoint, HitTestPosition,
    LineMetric, Text, TextAttribute, TextLayout, TextLayoutBuilder, TextStorage,
};

use font_kit::{
    error::SelectionError,
    family_name::FamilyName as FkFamilyName,
    loaders::freetype::Font,
    properties::{Properties as FkProps, Style as FkStyle, Weight as FkFontWeight},
    source::SystemSource,
};
use unicode_segmentation::UnicodeSegmentation;

use self::grapheme::{get_grapheme_boundaries, point_x_in_grapheme};

const FT_KEY: UserDataKey<Font> = UserDataKey::new();

/// Right now, we don't need any state, as the "toy text API" treats the
/// access to system font information as a global. This will change.
// we use a phantom lifetime here to match the API of the d2d backend,
// and the likely API of something with access to system font information.
#[derive(Clone)]
pub struct CairoText {
    /// An object used to search for fonts on the system.
    source: Arc<SystemSource>,
}

impl CairoText {
    pub fn new(source: SystemSource) -> Self {
        CairoText {
            source: Arc::new(source),
        }
    }
}

#[derive(Clone)]
struct CairoFont {
    family: FontFamily,
}

#[derive(Clone)]
pub struct CairoTextLayout {
    // we currently don't handle range attributes, so we stash the default
    // color here and then just grab it when we draw ourselves.
    pub(crate) fg_color: Color,
    size: Size,
    pub(crate) font: ScaledFont,
    pub(crate) text: Rc<dyn TextStorage>,

    // currently calculated on build
    pub(crate) line_metrics: Vec<LineMetric>,
}

pub struct CairoTextLayoutBuilder {
    text: Rc<dyn TextStorage>,
    defaults: util::LayoutDefaults,
    width_constraint: f64,
    source: Arc<SystemSource>,
}

impl Text for CairoText {
    type TextLayout = CairoTextLayout;
    type TextLayoutBuilder = CairoTextLayoutBuilder;

    fn font_family(&mut self, family_name: &str) -> Option<FontFamily> {
        match self.source.select_family_by_name(family_name) {
            Ok(_handle) => Some(FontFamily::new_unchecked(family_name)),
            Err(SelectionError::NotFound) => None,
            Err(e) => {
                eprintln!("font loading error: {}", e);
                None
            }
        }
    }

    fn load_font(&mut self, _data: &[u8]) -> Result<FontFamily, Error> {
        Err(Error::NotSupported)
    }

    fn new_text_layout(&mut self, text: impl TextStorage) -> Self::TextLayoutBuilder {
        CairoTextLayoutBuilder {
            defaults: util::LayoutDefaults::default(),
            text: Rc::new(text),
            width_constraint: f64::INFINITY,
            source: self.source.clone(),
        }
    }
}

impl CairoFont {
    pub(crate) fn new(family: FontFamily) -> Self {
        CairoFont { family }
    }

    #[cfg(test)]
    pub(crate) fn resolve_simple(&self, size: f64) -> ScaledFont {
        self.resolve(size, FontStyle::Normal, FontWeight::Normal)
    }

    /// Create a ScaledFont for this family.
    pub(crate) fn resolve(
        &self,
        size: f64,
        style: FontStyle,
        weight: FontWeight,
        source: Arc<SystemSource>,
    ) -> ScaledFont {
        let family_name = fk_family_name(&self.family);
        let ft_font_face = Rc::new(
            source
                .select_best_match(&[family_name], &fk_props(style, weight))
                .unwrap()
                .load()
                .unwrap(),
        );
        let font_face = unsafe {
            let face = FontFace::create_from_ft(ft_font_face.native_font());
            // make sure the freetype font hangs around for as long as the cairo font.
            face.set_user_data(&FT_KEY, ft_font_face);
            face
        };
        let font_matrix = scale_matrix(size);
        let ctm = scale_matrix(1.0);
        let options = FontOptions::default();
        ScaledFont::new(&font_face, &font_matrix, &ctm, &options)
    }
}

impl TextLayoutBuilder for CairoTextLayoutBuilder {
    type Out = CairoTextLayout;

    fn max_width(mut self, width: f64) -> Self {
        self.width_constraint = width;
        self
    }

    fn alignment(self, _alignment: piet::TextAlignment) -> Self {
        // TextAlignment is not supported by cairo toy text.
        self
    }

    fn default_attribute(mut self, attribute: impl Into<TextAttribute>) -> Self {
        self.defaults.set(attribute);
        self
    }

    fn range_attribute(
        self,
        _range: impl RangeBounds<usize>,
        _attribute: impl Into<TextAttribute>,
    ) -> Self {
        self
    }

    fn build(self) -> Result<Self::Out, Error> {
        // set our default font
        let font = CairoFont::new(self.defaults.font.clone());
        let size = self.defaults.font_size;

        let scaled_font = font.resolve(
            self.defaults.font_size,
            self.defaults.style,
            self.defaults.weight,
            self.source,
        );

        // invalid until update_width() is called
        let mut layout = CairoTextLayout {
            fg_color: self.defaults.fg_color,
            font: scaled_font,
            size: Size::ZERO,
            line_metrics: Vec::new(),
            text: self.text,
        };

        layout.update_width(self.width_constraint)?;
        Ok(layout)
    }
}

impl TextLayout for CairoTextLayout {
    fn size(&self) -> Size {
        self.size
    }

    fn image_bounds(&self) -> Rect {
        self.size.to_rect()
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        self.line_metrics
            .get(line_number)
            .map(|lm| &self.text[lm.range()])
    }

    fn line_metric(&self, line_number: usize) -> Option<LineMetric> {
        self.line_metrics.get(line_number).cloned()
    }

    fn line_count(&self) -> usize {
        self.line_metrics.len()
    }

    fn hit_test_point(&self, point: Point) -> HitTestPoint {
        // internal logic is using grapheme clusters, but return the text position associated
        // with the border of the grapheme cluster.

        // null case
        if self.text.is_empty() {
            return HitTestPoint::default();
        }

        let height = self
            .line_metrics
            .last()
            .map(|lm| lm.y_offset + lm.height)
            .unwrap_or(0.0);

        // determine whether this click is within the y bounds of the layout,
        // and what line it coorresponds to. (For points above and below the layout,
        // we hittest the first and last lines respectively.)
        let (y_inside, lm) = if point.y < 0. {
            (false, self.line_metrics.first().unwrap())
        } else if point.y >= height {
            (false, self.line_metrics.last().unwrap())
        } else {
            let line = self
                .line_metrics
                .iter()
                .find(|l| point.y >= l.y_offset && point.y < l.y_offset + l.height)
                .unwrap();
            (true, line)
        };

        // Trailing whitespace is remove for the line
        let line = &self.text[lm.range()];

        let mut htp = hit_test_line_point(&self.font, line, point);
        htp.idx += lm.start_offset;
        if htp.idx == lm.end_offset {
            htp.idx -= util::trailing_nlf(line).unwrap_or(0);
        }
        htp.is_inside &= y_inside;
        htp
    }

    fn hit_test_text_position(&self, idx: usize) -> HitTestPosition {
        let idx = idx.min(self.text.len());
        assert!(self.text.is_char_boundary(idx));

        if idx == 0 && self.text.is_empty() {
            return HitTestPosition::new(Point::new(0., self.font.extents().ascent), 0);
        }

        // first need to find line it's on, and get line start offset
        let line_num = util::line_number_for_position(&self.line_metrics, idx);
        let lm = self.line_metrics.get(line_num).cloned().unwrap();

        let y_pos = lm.y_offset + lm.baseline;

        // Then for the line, do text position
        // Trailing whitespace is removed for the line
        let line = &self.text[lm.range()];
        let line_position = idx - lm.start_offset;

        let x_pos = hit_test_line_position(&self.font, line, line_position);
        HitTestPosition::new(Point::new(x_pos, y_pos), line_num)
    }
}

impl CairoTextLayout {
    fn update_width(&mut self, new_width: impl Into<Option<f64>>) -> Result<(), Error> {
        let new_width = new_width.into().unwrap_or(std::f64::INFINITY);

        self.line_metrics = lines::calculate_line_metrics(&self.text, &self.font, new_width);
        if self.text.is_empty() {
            self.line_metrics.push(LineMetric {
                baseline: self.font.extents().ascent,
                height: self.font.extents().height,
                ..Default::default()
            })
        } else if util::trailing_nlf(&self.text).is_some() {
            let newline_eof = self
                .line_metrics
                .last()
                .map(|lm| LineMetric {
                    start_offset: self.text.len(),
                    end_offset: self.text.len(),
                    height: lm.height,
                    baseline: lm.baseline,
                    y_offset: lm.y_offset + lm.height,
                    trailing_whitespace: 0,
                })
                .unwrap();
            self.line_metrics.push(newline_eof);
        }

        let width = self
            .line_metrics
            .iter()
            .map(|lm| self.font.text_extents(&self.text[lm.range()]).x_advance)
            .fold(0.0, |a: f64, b| a.max(b));

        let height = self
            .line_metrics
            .last()
            .map(|l| l.y_offset + l.height)
            .unwrap_or_else(|| self.font.extents().height);
        self.size = Size::new(width, height);

        Ok(())
    }
}

// NOTE this is the same as the old, non-line-aware version of hit_test_point
// Future: instead of passing Font, should there be some other line-level text layout?
fn hit_test_line_point(font: &ScaledFont, text: &str, point: Point) -> HitTestPoint {
    // null case
    if text.is_empty() {
        return HitTestPoint::default();
    }

    // get bounds
    // TODO handle if string is not null yet count is 0?
    let end = UnicodeSegmentation::graphemes(text, true).count() - 1;
    let end_bounds = match get_grapheme_boundaries(font, text, end) {
        Some(bounds) => bounds,
        None => return HitTestPoint::default(),
    };

    let start = 0;
    let start_bounds = match get_grapheme_boundaries(font, text, start) {
        Some(bounds) => bounds,
        None => return HitTestPoint::default(),
    };

    // first test beyond ends
    if point.x > end_bounds.trailing {
        return HitTestPoint::new(text.len(), false);
    }
    if point.x <= start_bounds.leading {
        return HitTestPoint::default();
    }

    // then test the beginning and end (common cases)
    if let Some(hit) = point_x_in_grapheme(point.x, &start_bounds) {
        return hit;
    }
    if let Some(hit) = point_x_in_grapheme(point.x, &end_bounds) {
        return hit;
    }

    // Now that we know it's not beginning or end, begin binary search.
    // Iterative style
    let mut left = start;
    let mut right = end;
    loop {
        // pick halfway point
        let middle = left + ((right - left) / 2);

        let grapheme_bounds = match get_grapheme_boundaries(font, text, middle) {
            Some(bounds) => bounds,
            None => return HitTestPoint::default(),
        };

        if let Some(hit) = point_x_in_grapheme(point.x, &grapheme_bounds) {
            return hit;
        }

        // since it's not a hit, check if closer to start or finish
        // and move the appropriate search boundary
        if point.x < grapheme_bounds.leading {
            right = middle;
        } else if point.x > grapheme_bounds.trailing {
            left = middle + 1;
        } else {
            unreachable!("hit_test_point conditional is exhaustive");
        }
    }
}

// NOTE this is the same as the old, non-line-aware version of hit_test_text_position.
// Future: instead of passing Font, should there be some other line-level text layout?
fn hit_test_line_position(font: &ScaledFont, text: &str, text_position: usize) -> f64 {
    // Using substrings with unicode grapheme awareness

    let text_len = text.len();

    if text_position == 0 {
        return 0.0;
    }

    if text_position as usize >= text_len {
        return font.text_extents(&text).x_advance;
    }

    // Already checked that text_position > 0 and text_position < count.
    // If text position is not at a grapheme boundary, use the text position of current
    // grapheme cluster. But return the original text position
    // Use the indices (byte offset, which for our purposes = utf8 code units).
    let grapheme_indices = UnicodeSegmentation::grapheme_indices(text, true)
        .take_while(|(byte_idx, _s)| text_position >= *byte_idx);

    grapheme_indices
        .last()
        .map(|(idx, _)| font.text_extents(&text[..idx]).x_advance)
        .unwrap_or_else(|| font.text_extents(&text).x_advance)
}

fn scale_matrix(scale: f64) -> Matrix {
    Matrix {
        xx: scale,
        yx: 0.0,
        xy: 0.0,
        yy: scale,
        x0: 0.0,
        y0: 0.0,
    }
}

fn fk_props(style: FontStyle, weight: FontWeight) -> FkProps {
    let mut props = FkProps::new();
    props.style(fk_style(style));
    props.weight(fk_weight(weight));
    props
}

fn fk_style(style: FontStyle) -> FkStyle {
    match style {
        FontStyle::Regular => FkStyle::Normal,
        FontStyle::Italic => FkStyle::Italic,
    }
}

fn fk_weight(weight: FontWeight) -> FkFontWeight {
    FkFontWeight(weight.to_raw() as f32)
}

fn fk_family_name(family: &FontFamily) -> FkFamilyName {
    if *family == FontFamily::SANS_SERIF || *family == FontFamily::SYSTEM_UI {
        FkFamilyName::SansSerif
    } else if *family == FontFamily::SERIF {
        FkFamilyName::Serif
    } else if *family == FontFamily::MONOSPACE {
        FkFamilyName::Monospace
    } else {
        FkFamilyName::Title(family.name().to_owned())
    }
}

#[cfg(test)]
mod test;
