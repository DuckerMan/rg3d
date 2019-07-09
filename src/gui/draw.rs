use crate::math::vec2::Vec2;
use crate::math::Rect;
use crate::gui::{Thickness, HorizontalAlignment, VerticalAlignment};
use crate::resource::ttf::Font;
use std::os::raw::c_void;

#[derive(Copy, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn white() -> Color {
        Color { r: 255, g: 255, b: 255, a: 255 }
    }

    pub fn black() -> Color {
        Color { r: 0, g: 0, b: 0, a: 255 }
    }
}

#[repr(C)]
pub struct Vertex {
    pos: Vec2,
    tex_coord: Vec2,
    color: Color,
}

impl Vertex {
    fn new(pos: Vec2, tex_coord: Vec2, color: Color) -> Vertex {
        Vertex {
            pos,
            tex_coord,
            color,
        }
    }
}

pub enum CommandKind {
    Geometry,
    Clip,
}

pub struct Command {
    kind: CommandKind,
    texture: u32,
    index_offset: usize,
    triangle_count: usize,
    nesting: u8,
}

impl Command {
    #[inline]
    pub fn get_kind(&self) -> &CommandKind {
        &self.kind
    }

    #[inline]
    pub fn get_texture(&self) -> u32 {
        self.texture
    }

    #[inline]
    pub fn get_index_offset(&self) -> usize {
        self.index_offset
    }

    #[inline]
    pub fn get_triangle_count(&self) -> usize {
        self.triangle_count
    }

    #[inline]
    pub fn get_nesting(&self) -> u8 {
        self.nesting
    }
}

pub struct DrawingContext {
    vertex_buffer: Vec<Vertex>,
    index_buffer: Vec<i32>,
    command_buffer: Vec<Command>,
    clip_cmd_stack: Vec<i32>,
    opacity_stack: Vec<f32>,
    triangles_to_commit: usize,
    current_nesting: u8,
}

struct TextGlyph {
    bounds: Rect<f32>,
    tex_coords: [Vec2; 4],
    color: Color,
}

#[derive(Copy, Clone)]
struct TextLine {
    begin: usize,
    end: usize,
    width: f32,
    x_offset: f32
}

impl TextLine {
    fn new() -> TextLine {
        TextLine {
            begin: 0,
            end: 0,
            width: 0.0,
            x_offset: 0.0
        }
    }
}

pub struct FormattedText {
    texture: u32,
    /// Text in UTF32 format.
    text: Vec<u32>,
    /// Temporary buffer used to split text on lines. We need it to reduce memory allocations
    /// when we changing text too frequently, here we sacrifice some memory in order to get
    /// more performance.
    lines: Vec<TextLine>,
    /// Final glyphs for draw buffer.
    glyphs: Vec<TextGlyph>,
}

impl FormattedText {
    fn new() -> FormattedText {
        FormattedText {
            text: Vec::new(),
            texture: 0,
            glyphs: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn build(&mut self, text: &str, font: &Font, bounds: &Rect<f32>, color: Color,
             vertical_alignment: VerticalAlignment, horizontal_alignment: HorizontalAlignment) {
        // Convert text to UTF32.
        self.text.clear();
        for code in text.chars().map(|c| c as u32) {
            self.text.push(code);
        }

        // Split on lines.
        let mut total_height = 0.0;
        let mut current_line = TextLine::new();
        self.lines.clear();
        for (i, code) in self.text.iter().enumerate() {
            let advance =
                match font.get_glyph(*code) {
                    Some(glyph) => glyph.get_advance(),
                    None => font.get_height()
                };
            let is_new_line = *code == b'\n' as u32 || *code == '\r' as u32;
            let new_width = current_line.width + advance;
            if new_width > bounds.w || is_new_line {
                self.lines.push(current_line.clone());
                current_line.begin = if is_new_line { i + 1 } else { i };
                current_line.end = current_line.begin + 1;
                current_line.width = advance;
                total_height += font.get_ascender();
            } else {
                current_line.width = new_width;
                current_line.end += 1;
            }
        }
        // Commit rest of text.
        if current_line.begin != current_line.end {
            current_line.end = self.text.len();
            self.lines.push(current_line);
            total_height += font.get_ascender();
        }

        // Align lines according to desired alignment.
        for line in self.lines.iter_mut() {
            match horizontal_alignment {
                HorizontalAlignment::Left => line.x_offset = 0.0,
                HorizontalAlignment::Center => line.x_offset = 0.5 * (bounds.w - line.width),
                HorizontalAlignment::Right => line.x_offset = bounds.w - line.width,
                HorizontalAlignment::Stretch => line.x_offset = 0.0
            }
        }

        self.texture = font.get_texture_id();

        // Generate glyphs for each text line.
        self.glyphs.clear();

        let cursor_y_start = match vertical_alignment {
            VerticalAlignment::Top => bounds.y,
            VerticalAlignment::Center => bounds.y + (bounds.h - total_height) * 0.5,
            VerticalAlignment::Bottom => bounds.y + bounds.h - total_height,
            VerticalAlignment::Stretch => bounds.y
        };

        let mut cursor = Vec2::make(bounds.x, cursor_y_start);
        for line in self.lines.iter() {
            cursor.x = bounds.x + line.x_offset;

            for code_index in line.begin..line.end {
                let code = self.text[code_index];

                match font.get_glyph(code) {
                    Some(glyph) => {
                        // Insert glyph
                        if glyph.has_outline() {
                            let rect = Rect {
                                x: cursor.x + glyph.get_bitmap_left(),
                                y: cursor.y + font.get_ascender() - glyph.get_bitmap_top() - glyph.get_bitmap_height(),
                                w: glyph.get_bitmap_width(),
                                h: glyph.get_bitmap_height(),
                            };
                            self.glyphs.push(TextGlyph {
                                bounds: rect,
                                tex_coords: glyph.get_tex_coords().clone(),
                                color,
                            });
                        }
                        cursor.x += glyph.get_advance();
                    }
                    None => {
                        // Insert invalid symbol
                        let rect = Rect {
                            x: cursor.x,
                            y: cursor.y + font.get_ascender(),
                            w: font.get_height(),
                            h: font.get_height(),
                        };
                        self.glyphs.push(TextGlyph {
                            bounds: rect,
                            tex_coords: [Vec2::new(); 4],
                            color,
                        });
                        cursor.x += rect.w;
                    }
                }
            }

            cursor.y += font.get_ascender();
        }
    }
}

pub struct FormattedTextBuilder<'a> {
    color: Color,
    bounds: Rect<f32>,
    text: Option<&'a str>,
    font: Option<&'a Font>,
    formatted_text: FormattedText,
    vertical_alignment: VerticalAlignment,
    horizontal_alignment: HorizontalAlignment,
}

impl<'a> FormattedTextBuilder<'a> {
    /// Creates new formatted text builder with default parameters.
    pub fn new() -> FormattedTextBuilder<'a> {
        FormattedTextBuilder {
            font: None,
            text: None,
            formatted_text: FormattedText::new(),
            horizontal_alignment: HorizontalAlignment::Left,
            vertical_alignment: VerticalAlignment::Top,
            color: Color::white(),
            bounds: Rect::new(0.0, 0.0, 128.0, 128.0),
        }
    }

    /// Creates new formatted text builder that will reuse existing
    /// buffers from existing formatted text. This is very useful to
    /// reduce memory allocations.
    pub fn reuse(formatted_text: FormattedText) -> FormattedTextBuilder<'a> {
        FormattedTextBuilder {
            font: None,
            text: None,
            formatted_text: FormattedText {
                // Take buffers out and reuse them so no need to allocate new
                // buffers every time when need to change a text.
                text: formatted_text.text,
                lines: formatted_text.lines,
                glyphs: formatted_text.glyphs,
                texture: 0,
            },
            horizontal_alignment: HorizontalAlignment::Left,
            vertical_alignment: VerticalAlignment::Top,
            color: Color::white(),
            bounds: Rect::new(0.0, 0.0, 128.0, 128.0),
        }
    }

    pub fn with_font(mut self, font: &'a Font) -> Self {
        self.font = Some(font);
        self
    }

    pub fn with_vertical_alignment(mut self, vertical_alignment: VerticalAlignment) -> Self {
        self.vertical_alignment = vertical_alignment;
        self
    }

    pub fn with_horizontal_alignment(mut self, horizontal_alignment: HorizontalAlignment) -> Self {
        self.horizontal_alignment = horizontal_alignment;
        self
    }

    pub fn with_text(mut self, text: &'a str) -> Self {
        self.text = Some(text);
        self
    }

    pub fn with_bounds(mut self, bounds: Rect<f32>) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn build(mut self) -> FormattedText {
        if let Some(text) = self.text {
            if let Some(font) = self.font {
                self.formatted_text.build(
                    text,
                    font,
                    &self.bounds,
                    self.color,
                    self.vertical_alignment,
                    self.horizontal_alignment
                );
            }
        }

        self.formatted_text
    }
}

fn get_line_thickness_vector(a: &Vec2, b: &Vec2, thickness: f32) -> Vec2 {
    if let Some(dir) = (*b - *a).normalized() {
        dir.perpendicular().scale(thickness * 0.5)
    } else {
        Vec2::new()
    }
}

impl DrawingContext {
    pub fn new() -> DrawingContext {
        DrawingContext {
            vertex_buffer: Vec::new(),
            index_buffer: Vec::new(),
            command_buffer: Vec::new(),
            clip_cmd_stack: Vec::new(),
            opacity_stack: Vec::new(),
            triangles_to_commit: 0,
            current_nesting: 0,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.vertex_buffer.clear();
        self.index_buffer.clear();
        self.command_buffer.clear();
        self.clip_cmd_stack.clear();
        self.opacity_stack.clear();
        self.triangles_to_commit = 0;
        self.current_nesting = 0;
    }


    #[inline]
    pub fn get_command_buffer(&self) -> &[Command] {
        self.command_buffer.as_slice()
    }

    #[inline]
    pub fn get_vertices(&self) -> &[Vertex] {
        self.vertex_buffer.as_slice()
    }

    #[inline]
    pub fn get_indices(&self) -> &[i32] {
        self.index_buffer.as_slice()
    }

    #[inline]
    pub fn get_vertices_ptr(&self) -> *const c_void {
        self.vertex_buffer.as_ptr() as *const c_void
    }

    #[inline]
    pub fn get_indices_ptr(&self) -> *const c_void {
        self.index_buffer.as_ptr() as *const c_void
    }

    #[inline]
    pub fn get_vertices_bytes(&self) -> isize {
        (self.vertex_buffer.len() * std::mem::size_of::<Vertex>()) as isize
    }

    #[inline]
    pub fn get_vertex_size(&self) -> i32 {
        std::mem::size_of::<Vertex>() as i32
    }

    #[inline]
    pub fn get_index_size(&self) -> i32 {
        std::mem::size_of::<i32>() as i32
    }

    #[inline]
    pub fn get_indices_bytes(&self) -> isize {
        (self.index_buffer.len() * std::mem::size_of::<i32>()) as isize
    }

    #[inline]
    fn push_vertex(&mut self, pos: Vec2, tex_coord: Vec2, color: Color) {
        self.vertex_buffer.push(Vertex::new(pos, tex_coord, color));
    }

    #[inline]
    fn push_triangle(&mut self, a: i32, b: i32, c: i32) {
        self.index_buffer.push(a);
        self.index_buffer.push(b);
        self.index_buffer.push(c);
        self.triangles_to_commit += 1;
    }

    #[inline]
    fn get_index_origin(&self) -> i32 {
        if self.index_buffer.len() > 0 {
            self.index_buffer.last().unwrap() + 1
        } else {
            0
        }
    }

    pub fn push_line(&mut self, a: &Vec2, b: &Vec2, thickness: f32, color: Color) {
        let perp = get_line_thickness_vector(a, b, thickness);
        self.push_vertex(*a - perp, Vec2::make(0.0, 0.0), color);
        self.push_vertex(*b - perp, Vec2::make(1.0, 0.0), color);
        self.push_vertex(*a + perp, Vec2::make(1.0, 1.0), color);
        self.push_vertex(*b + perp, Vec2::make(0.0, 1.0), color);

        let index = self.get_index_origin();
        self.push_triangle(index, index + 1, index + 2);
        self.push_triangle(index, index + 2, index + 3);
    }

    pub fn push_rect(&mut self, rect: &Rect<f32>, thickness: f32, color: Color) {
        let offset = thickness * 0.5;

        let left_top = Vec2::make(rect.x + offset, rect.y + thickness);
        let right_top = Vec2::make(rect.x + rect.w - offset, rect.y + thickness);
        let right_bottom = Vec2::make(rect.x + rect.w - offset, rect.y + rect.h - thickness);
        let left_bottom = Vec2::make(rect.x + offset, rect.y + rect.h - thickness);
        let left_top_off = Vec2::make(rect.x, rect.y + offset);
        let right_top_off = Vec2::make(rect.x + rect.w, rect.y + offset);
        let right_bottom_off = Vec2::make(rect.x + rect.w, rect.y + rect.h - offset);
        let left_bottom_off = Vec2::make(rect.x, rect.y + rect.h - offset);

        // Horizontal lines
        self.push_line(&left_top_off, &right_top_off, thickness, color);
        self.push_line(&right_bottom_off, &left_bottom_off, thickness, color);

        // Vertical lines
        self.push_line(&right_top, &right_bottom, thickness, color);
        self.push_line(&left_bottom, &left_top, thickness, color);
    }

    pub fn push_rect_vary(&mut self, rect: &Rect<f32>, thickness: Thickness, color: Color) {
        let left_top = Vec2::make(rect.x + thickness.left * 0.5, rect.y + thickness.top);
        let right_top = Vec2::make(rect.x + rect.w - thickness.right * 0.5, rect.y + thickness.top);
        let right_bottom = Vec2::make(rect.x + rect.w - thickness.right * 0.5, rect.y + rect.h - thickness.bottom);
        let left_bottom = Vec2::make(rect.x + thickness.left * 0.5, rect.y + rect.h - thickness.bottom);
        let left_top_off = Vec2::make(rect.x, rect.y + thickness.top * 0.5);
        let right_top_off = Vec2::make(rect.x + rect.w, rect.y + thickness.top * 0.5);
        let right_bottom_off = Vec2::make(rect.x + rect.w, rect.y + rect.h - thickness.bottom * 0.5);
        let left_bottom_off = Vec2::make(rect.x, rect.y + rect.h - thickness.bottom * 0.5);

        // Horizontal lines
        self.push_line(&left_top_off, &right_top_off, thickness.top, color);
        self.push_line(&right_bottom_off, &left_bottom_off, thickness.bottom, color);

        // Vertical lines
        self.push_line(&right_top, &right_bottom, thickness.right, color);
        self.push_line(&left_bottom, &left_top, thickness.left, color);
    }

    pub fn push_rect_filled(&mut self, rect: &Rect<f32>, tex_coords: Option<&[Vec2; 4]>, color: Color) {
        self.push_vertex(Vec2::make(rect.x, rect.y), tex_coords.map_or(Vec2::make(0.0, 0.0), |t| t[0]), color);
        self.push_vertex(Vec2::make(rect.x + rect.w, rect.y), tex_coords.map_or(Vec2::make(1.0, 0.0), |t| t[1]), color);
        self.push_vertex(Vec2::make(rect.x + rect.w, rect.y + rect.h), tex_coords.map_or(Vec2::make(1.0, 1.0), |t| t[2]), color);
        self.push_vertex(Vec2::make(rect.x, rect.y + rect.h), tex_coords.map_or(Vec2::make(0.0, 1.0), |t| t[3]), color);

        let index = self.get_index_origin();
        self.push_triangle(index, index + 1, index + 2);
        self.push_triangle(index, index + 2, index + 3);
    }

    pub fn commit(&mut self, kind: CommandKind, texture: u32) {
        if self.triangles_to_commit > 0 {
            self.command_buffer.push(Command {
                kind,
                texture,
                nesting: self.current_nesting,
                index_offset: if self.index_buffer.len() > 0 {
                    self.index_buffer.len() - self.triangles_to_commit * 3
                } else {
                    0
                },
                triangle_count: self.triangles_to_commit,
            });
            self.triangles_to_commit = 0;
        }
    }

    pub fn draw_text(&mut self, formatted_text: &FormattedText) {
        for element in formatted_text.glyphs.iter() {
            self.push_rect_filled(&element.bounds, Some(&element.tex_coords), element.color);
        }
        self.commit(CommandKind::Geometry, formatted_text.texture);
    }
}