//! Turns the page layout into a PDF with krilla. Both fonts are embedded (as subsets), so the file
//! looks the same everywhere and needs nothing from the computer that opens it.

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::{Document, SerializeSettings};

use super::layout::{Color, Doc, Op, BOLD_FONT, REGULAR_FONT};
use crate::vault::AppError;

fn fill(color: Color) -> Fill {
    Fill { paint: rgb::Color::new(color.0, color.1, color.2).into(), opacity: NormalizedF32::ONE, rule: Default::default() }
}

fn stroke(color: Color, width: f32) -> Stroke {
    Stroke { paint: rgb::Color::new(color.0, color.1, color.2).into(), width, ..Default::default() }
}

fn font(data: &'static [u8]) -> Result<Font, AppError> {
    Font::new(data.to_vec().into(), 0).ok_or(AppError::Internal)
}

/// `compress` is on for real reports. The tests turn it off so the page content can be read as text.
pub fn pdf(doc: &Doc, title: &str, compress: bool) -> Result<Vec<u8>, AppError> {
    let regular = font(REGULAR_FONT)?;
    let bold = font(BOLD_FONT)?;
    let mut document = Document::new_with(SerializeSettings { compress_content_streams: compress, ..Default::default() });
    // No creation date and no author: the file says only what it is.
    document.set_metadata(Metadata::new().title(title.to_string()).creator("meowNY".to_string()));

    for ops in &doc.pages {
        let settings = PageSettings::from_wh(doc.width, doc.height).ok_or(AppError::Internal)?;
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        for op in ops {
            match op {
                Op::Text { x, y, size, bold: is_bold, color, text } => {
                    surface.set_stroke(None);
                    surface.set_fill(Some(fill(*color)));
                    let face = if *is_bold { bold.clone() } else { regular.clone() };
                    surface.draw_text(Point::from_xy(*x, *y), face, *size, text, false, TextDirection::LeftToRight);
                }
                Op::Rect { x, y, w, h, color } => {
                    let rect = Rect::from_xywh(*x, *y, *w, *h).ok_or(AppError::Internal)?;
                    let mut path = PathBuilder::new();
                    path.push_rect(rect);
                    surface.set_stroke(None);
                    surface.set_fill(Some(fill(*color)));
                    surface.draw_path(&path.finish().ok_or(AppError::Internal)?);
                }
                Op::Poly { points, fill: color, outline } => {
                    let mut path = PathBuilder::new();
                    for (i, (x, y)) in points.iter().enumerate() {
                        if i == 0 {
                            path.move_to(*x, *y);
                        } else {
                            path.line_to(*x, *y);
                        }
                    }
                    path.close();
                    surface.set_fill(Some(fill(*color)));
                    surface.set_stroke(outline.map(|(c, w)| stroke(c, w)));
                    surface.draw_path(&path.finish().ok_or(AppError::Internal)?);
                }
                Op::Line { x1, y1, x2, y2, width, color } => {
                    let mut path = PathBuilder::new();
                    path.move_to(*x1, *y1);
                    path.line_to(*x2, *y2);
                    surface.set_fill(None);
                    surface.set_stroke(Some(stroke(*color, *width)));
                    surface.draw_path(&path.finish().ok_or(AppError::Internal)?);
                }
            }
        }
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|_| AppError::Internal)
}
