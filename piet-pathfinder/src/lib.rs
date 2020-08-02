use ::{
    image::RgbaImage,
    pathfinder_canvas::{
        CanvasImageSource, CanvasRenderingContext2D, ColorU, FillRule, FillStyle,
        ImageSmoothingQuality, LineCap as PfLineCap, LineJoin as PfLineJoin, Path2D, RectF,
        Transform2F, Vector2F,
    },
    pathfinder_content::{
        gradient::{ColorStop, Gradient},
        pattern::{Image, Pattern},
    },
    piet::{
        kurbo::{Affine, Circle, Line, PathEl, Point, Rect, Shape},
        Color, Error, FixedGradient, FixedLinearGradient, FixedRadialGradient, GradientStop,
        ImageFormat, InterpolationMode, IntoBrush, LineCap, LineJoin, RenderContext, RoundFrom,
        RoundInto, StrokeStyle,
    },
    std::{borrow::Cow, convert::TryInto, f32::consts::PI},
};

pub mod text;

pub struct PfContext<'a> {
    render_ctx: &'a mut CanvasRenderingContext2D,
    text: text::PfText,
}

impl<'a> PfContext<'a> {
    pub fn new(render_ctx: &'a mut CanvasRenderingContext2D) -> Self {
        PfContext {
            render_ctx,
            text: text::PfText,
        }
    }
}

impl RenderContext for PfContext<'_> {
    type Brush = FillStyle;
    // TODO the whole text thing needs overhauling, including allowing the user to select fonts.
    type Text = text::PfText;
    type TextLayout = text::PfTextLayout;
    type Image = PfImage;

    fn status(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn solid_brush(&mut self, color: Color) -> Self::Brush {
        FillStyle::Color(map_color(color))
    }

    fn gradient(&mut self, gradient: impl Into<FixedGradient>) -> Result<Self::Brush, Error> {
        Ok(match gradient.into() {
            FixedGradient::Linear(grad) => lineargradient_to_fillstyle(grad),
            FixedGradient::Radial(grad) => radialgradient_to_fillstyle(grad),
        })
    }

    fn clear(&mut self, color: Color) {
        // TODO here I'm just drawing a rectangle to cover any existing content. Might be a better
        // way to do it.
        let size = self.render_ctx.canvas().size();
        let brush = self.solid_brush(color);
        self.render_ctx.set_fill_style(brush);
        self.render_ctx
            .fill_rect(RectF::new(vec2f(0.0, 0.0), size.to_f32()));
    }

    fn stroke(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>, width: f64) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.render_ctx.set_stroke_style(brush.into_owned());
        self.render_ctx.set_line_width(width.round_into());
        self.render_ctx.set_line_cap(PfLineCap::Butt);
        self.render_ctx.set_line_join(PfLineJoin::Miter);
        self.render_ctx.set_miter_limit(10.0);
        self.render_ctx.set_line_dash(vec![]);
        self.render_ctx.set_line_dash_offset(0.0);
        self.render_ctx.stroke_path(shape_to_path2d(shape));
    }

    fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &impl IntoBrush<Self>,
        width: f64,
        style: &StrokeStyle,
    ) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.render_ctx.set_stroke_style(brush.into_owned());
        self.render_ctx.set_line_width(width.round_into());
        self.render_ctx
            .set_line_cap(linecap_into(style.line_cap.unwrap_or(LineCap::Butt)));
        self.render_ctx
            .set_line_join(linejoin_into(style.line_join.unwrap_or(LineJoin::Miter)));
        self.render_ctx.set_miter_limit(10.0);
        self.render_ctx.set_line_dash(vec![]);
        self.render_ctx.set_line_dash_offset(0.0);
        self.render_ctx.stroke_path(shape_to_path2d(shape));
    }

    fn fill(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.render_ctx.set_fill_style(brush.into_owned());
        self.render_ctx
            .fill_path(shape_to_path2d(shape), FillRule::Winding);
    }

    fn fill_even_odd(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.render_ctx.set_fill_style(brush.into_owned());
        self.render_ctx
            .fill_path(shape_to_path2d(shape), FillRule::EvenOdd);
    }

    fn clip(&mut self, shape: impl Shape) {
        self.render_ctx
            .clip_path(shape_to_path2d(shape), FillRule::Winding);
    }

    fn text(&mut self) -> &mut Self::Text {
        &mut self.text
    }

    fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<Point>) {
        let pos = pos.into();
        //self.render_ctx.set_font(layout.font.name.as_str());
        self.render_ctx.set_font_size(layout.font.size as f32);
        let metrics = self.render_ctx.measure_text(&layout.text);
        let bbox = Rect::new(
            pos.x - f64::round_from(metrics.actual_bounding_box_left),
            pos.x + f64::round_from(metrics.actual_bounding_box_right),
            pos.y - f64::round_from(metrics.actual_bounding_box_ascent),
            pos.y + f64::round_from(metrics.actual_bounding_box_descent),
        );
        let brush = layout.color.make_brush(self, || bbox);
        self.render_ctx.set_fill_style(brush.into_owned());
        self.render_ctx
            .fill_text(&layout.text, point_to_vec2f(pos.into()));
    }

    fn save(&mut self) -> Result<(), Error> {
        self.render_ctx.save();
        Ok(())
    }

    fn restore(&mut self) -> Result<(), Error> {
        self.render_ctx.restore();
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn transform(&mut self, transform: Affine) {
        self.render_ctx
            .set_transform(&affine_to_transform2f(transform))
    }

    fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: ImageFormat,
    ) -> Result<Self::Image, Error> {
        match format {
            ImageFormat::RgbaSeparate => Ok(PfImage(
                RgbaImage::from_raw(
                    width.try_into().ok().ok_or_else(not_supported)?,
                    height.try_into().ok().ok_or_else(not_supported)?,
                    buf.to_owned(),
                )
                .ok_or_else(invalid_input)?,
            )),
            _ => Err(not_supported()),
        }
    }

    fn draw_image(
        &mut self,
        image: &Self::Image,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        set_interpolation(self, interp);
        self.render_ctx
            .draw_image((*image).clone(), rect_to_rectf(dst_rect.into()))
    }

    fn draw_image_area(
        &mut self,
        image: &Self::Image,
        src_rect: impl Into<Rect>,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        set_interpolation(self, interp);
        self.render_ctx.draw_subimage(
            (*image).clone(),
            rect_to_rectf(src_rect.into()),
            rect_to_rectf(dst_rect.into()),
        );
    }

    fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
        todo!()
    }

    fn current_transform(&self) -> Affine {
        let t = self.render_ctx.transform();
        Affine::new([
            t.matrix.m11().into(),
            t.matrix.m21().into(),
            t.matrix.m12().into(),
            t.matrix.m22().into(),
            t.vector.x().into(),
            t.vector.y().into(),
        ])
    }
}

impl IntoBrush<PfContext<'_>> for FillStyle {
    fn make_brush(
        &self,
        ctx: &mut PfContext,
        bbox: impl FnOnce() -> Rect,
    ) -> Cow<<PfContext as RenderContext>::Brush> {
        Cow::Borrowed(self)
    }
}
#[derive(Debug, Clone)]
pub struct PfImage(pub image::RgbaImage);

impl CanvasImageSource for PfImage {
    fn to_pattern(
        self,
        dest_context: &mut CanvasRenderingContext2D,
        transform: Transform2F,
    ) -> Pattern {
        let mut p = Pattern::from_image(Image::from_image_buffer(self.0));
        p.apply_transform(transform);
        p
    }
}

// helpers

fn map_color(input: Color) -> ColorU {
    let (r, g, b, a) = input.as_rgba8();
    ColorU::new(r, g, b, a)
}

fn shape_to_path2d(input: impl Shape) -> Path2D {
    let mut path = Path2D::new();
    if let Some(Line { p0, p1 }) = input.as_line() {
        path.move_to(point_to_vec2f(p0));
        path.line_to(point_to_vec2f(p1));
    } else if let Some(r) = input.as_rect() {
        path.rect(rect_to_rectf(r));
    } else if let Some(Circle { center, radius }) = input.as_circle() {
        path.ellipse(
            point_to_vec2f(center),
            vec2f(radius, radius),
            0.0,
            0.0,
            2.0 * PI,
        );
    } else if let Some(els) = input.as_path_slice() {
        path_el_iter(&mut path, els.iter().map(|el| *el));
    } else {
        path_el_iter(&mut path, input.to_bez_path(0.1));
    }
    path
}

fn path_el_iter(path: &mut Path2D, iter: impl Iterator<Item = PathEl>) {
    let mut last_move_to: Vector2F = vec2f(0.0, 0.0);
    for el in iter {
        match el {
            PathEl::MoveTo(p) => {
                let p = point_to_vec2f(p);
                last_move_to = p;
                path.move_to(p)
            }
            PathEl::LineTo(p) => path.line_to(point_to_vec2f(p)),
            PathEl::QuadTo(p0, p1) => {
                path.quadratic_curve_to(point_to_vec2f(p0), point_to_vec2f(p1))
            }
            PathEl::CurveTo(p0, p1, p2) => {
                path.bezier_curve_to(point_to_vec2f(p0), point_to_vec2f(p1), point_to_vec2f(p2))
            }
            PathEl::ClosePath => path.line_to(last_move_to),
        }
    }
}

#[inline]
fn vec2f(x: f64, y: f64) -> Vector2F {
    Vector2F::new(x.round_into(), y.round_into())
}

#[inline]
fn point_to_vec2f(p: Point) -> Vector2F {
    vec2f(p.x, p.y)
}

#[inline]
fn affine_to_transform2f(t: Affine) -> Transform2F {
    todo!()
}

#[inline]
fn rect_to_rectf(r: Rect) -> RectF {
    let Rect { x0, y0, x1, y1 } = r;
    RectF::from_points(vec2f(x0, y0), vec2f(x1, y1))
}

#[inline]
fn lineargradient_to_fillstyle(grad: FixedLinearGradient) -> FillStyle {
    let mut output =
        Gradient::linear_from_points(point_to_vec2f(grad.start), point_to_vec2f(grad.end));
    for stop in grad.stops {
        output.add(gradientstop_to_colorstop(stop));
    }
    output.into()
}

#[inline]
fn radialgradient_to_fillstyle(grad: FixedRadialGradient) -> FillStyle {
    // TODO not sure how to implement this - I don't know how to match up the different models.
    todo!()
    /*
    let mut output = Gradient::radial();
    for stop in grad.stops {
        output.add(gradientstop_to_colorstop(stop));
    }
    output
    */
}

#[inline]
fn gradientstop_to_colorstop(stop: GradientStop) -> ColorStop {
    ColorStop {
        offset: stop.pos,
        color: map_color(stop.color),
    }
}

#[inline]
fn linecap_into(input: LineCap) -> PfLineCap {
    match input {
        LineCap::Butt => PfLineCap::Butt,
        LineCap::Square => PfLineCap::Square,
        LineCap::Round => PfLineCap::Round,
    }
}

#[inline]
fn linejoin_into(input: LineJoin) -> PfLineJoin {
    match input {
        LineJoin::Miter => PfLineJoin::Miter,
        LineJoin::Bevel => PfLineJoin::Bevel,
        LineJoin::Round => PfLineJoin::Round,
    }
}

#[inline]
fn set_interpolation(ctx: &mut PfContext, interp: InterpolationMode) {
    use InterpolationMode::*;
    match interp {
        NearestNeighbor => ctx.render_ctx.set_image_smoothing_enabled(false),
        Bilinear => {
            ctx.render_ctx.set_image_smoothing_enabled(true);
            // I'm assuming that the lowest quality is bilinear.
            ctx.render_ctx
                .set_image_smoothing_quality(ImageSmoothingQuality::Low);
        }
    }
}

#[inline]
fn not_supported() -> Error {
    piet::Error::NotSupported
}

#[inline]
fn invalid_input() -> Error {
    piet::Error::InvalidInput
}
