//! Text styles and the shared text system (font discovery, shaping, layout).

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use parley::{FontContext, FontFamilyName, GenericFamily, LayoutContext, StyleProperty};

use crate::Color;

/// Which typeface to use.
///
/// rugkit's design language uses rounded UI text for labels, buttons and headings and the
/// regular UI face for dense content (tables, inputs). On macOS these are SF Pro Rounded and
/// SF Pro, loaded from the system and never bundled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// Rounded UI face (SF Pro Rounded on macOS). The default.
    #[default]
    UiRounded,
    /// Regular UI face for dense content (SF Pro on macOS).
    Ui,
    /// Monospaced face.
    Monospace,
    /// A font family installed on the system, by name.
    Named(String),
}

/// Font weight on the CSS scale (`100.0` thin to `900.0` black).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontWeight(pub f32);

impl FontWeight {
    /// 300.
    pub const LIGHT: Self = Self(300.0);
    /// 400.
    pub const REGULAR: Self = Self(400.0);
    /// 500.
    pub const MEDIUM: Self = Self(500.0);
    /// 600.
    pub const SEMIBOLD: Self = Self(600.0);
    /// 700.
    pub const BOLD: Self = Self(700.0);
    /// 800.
    pub const HEAVY: Self = Self(800.0);
    /// 900.
    pub const BLACK: Self = Self(900.0);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::REGULAR
    }
}

/// How a run of text looks.
///
/// ```
/// use rugkit_render::{Color, FontWeight, TextStyle};
///
/// let title = TextStyle::new(32.0).weight(FontWeight::BOLD).color(Color::WHITE);
/// assert_eq!(title.size, 32.0);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Typeface.
    pub family: FontFamily,
    /// Font size in logical pixels.
    pub size: f32,
    /// Weight.
    pub weight: FontWeight,
    /// Text colour.
    pub color: Color,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    /// Extra space between letters, in logical pixels (negative tightens).
    pub letter_spacing: f32,
}

impl TextStyle {
    /// A rounded UI style at `size`, regular weight, black, line height 1.25.
    pub fn new(size: f32) -> Self {
        Self {
            family: FontFamily::UiRounded,
            size,
            weight: FontWeight::REGULAR,
            color: Color::BLACK,
            line_height: 1.25,
            letter_spacing: 0.0,
        }
    }

    /// Sets the typeface.
    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }

    /// Sets the weight.
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Sets the colour.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the line height as a multiple of the font size.
    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets the letter spacing in logical pixels.
    pub fn letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(15.0)
    }
}

const ROUNDED_FAMILY: &str = "rugkit UI Rounded";
const CACHE_SWEEP_FRAMES: u64 = 120;

/// Everything besides the text itself that affects a layout, with floats as bits so it can
/// be hashed and compared exactly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LayoutParams {
    family: FontFamily,
    size: u32,
    weight: u32,
    line_height: u32,
    letter_spacing: u32,
    max_width: Option<u64>,
    scale: u32,
}

impl LayoutParams {
    fn new(style: &TextStyle, max_width: Option<f64>, scale: f32) -> Self {
        Self {
            family: style.family.clone(),
            size: style.size.to_bits(),
            weight: style.weight.0.to_bits(),
            line_height: style.line_height.to_bits(),
            letter_spacing: style.letter_spacing.to_bits(),
            max_width: max_width.map(f64::to_bits),
            scale: scale.to_bits(),
        }
    }
}

struct CachedLayout {
    text: String,
    params: LayoutParams,
    layout: parley::Layout<()>,
    last_used: u64,
}
const UI_FAMILY: &str = "rugkit UI";

/// Owns fonts and shaping state. Create one per app and reuse it across frames.
pub struct TextSystem {
    pub(crate) fonts: FontContext,
    pub(crate) layouts: LayoutContext<()>,
    cache: HashMap<u64, CachedLayout>,
    frame: u64,
    has_rounded: bool,
    has_ui: bool,
}

impl std::fmt::Debug for TextSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextSystem")
            .field("has_rounded", &self.has_rounded)
            .field("has_ui", &self.has_ui)
            .finish_non_exhaustive()
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSystem {
    /// Discovers system fonts and registers rugkit's UI faces.
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        let has_rounded = register_system_face(&mut fonts, ROUNDED_FONT_PATH, ROUNDED_FAMILY);
        let has_ui = register_system_face(&mut fonts, UI_FONT_PATH, UI_FAMILY);
        Self {
            fonts,
            layouts: LayoutContext::new(),
            cache: HashMap::new(),
            frame: 0,
            has_rounded,
            has_ui,
        }
    }

    /// Advances the cache clock. Layouts unused for a while are evicted.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
        if self.frame.is_multiple_of(CACHE_SWEEP_FRAMES) {
            let frame = self.frame;
            self.cache
                .retain(|_, e| frame - e.last_used < CACHE_SWEEP_FRAMES);
        }
    }

    /// Returns the layout of `text` at `scale` physical pixels per logical pixel, reusing a
    /// cached layout when the same text and style were laid out recently.
    pub(crate) fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        scale: f32,
    ) -> &parley::Layout<()> {
        let params = LayoutParams::new(style, max_width, scale);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        params.hash(&mut hasher);
        let key = hasher.finish();
        let hit = self
            .cache
            .get(&key)
            .is_some_and(|e| e.text == text && e.params == params);
        if !hit {
            let layout = self.build_layout(text, style, max_width, scale);
            self.cache.insert(
                key,
                CachedLayout {
                    text: text.to_owned(),
                    params,
                    layout,
                    last_used: self.frame,
                },
            );
        }
        let entry = self.cache.get_mut(&key).expect("inserted above");
        entry.last_used = self.frame;
        &entry.layout
    }

    fn build_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        scale: f32,
    ) -> parley::Layout<()> {
        let families = self.families(&style.family);
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, scale, true);
        builder.push_default(StyleProperty::FontFamily(parley::FontFamily::List(
            Cow::Owned(families),
        )));
        builder.push_default(StyleProperty::FontSize(style.size));
        builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
            style.weight.0,
        )));
        builder.push_default(StyleProperty::LineHeight(
            parley::LineHeight::FontSizeRelative(style.line_height),
        ));
        builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
        let mut layout = builder.build(text);
        layout.break_all_lines(max_width.map(|w| (w * f64::from(scale)) as f32));
        layout
    }

    fn families(&self, family: &FontFamily) -> Vec<FontFamilyName<'static>> {
        let named = |n: &'static str| FontFamilyName::Named(Cow::Borrowed(n));
        let mut list = Vec::with_capacity(3);
        match family {
            FontFamily::UiRounded => {
                if self.has_rounded {
                    list.push(named(ROUNDED_FAMILY));
                }
                list.push(FontFamilyName::Generic(GenericFamily::UiRounded));
                list.push(FontFamilyName::Generic(GenericFamily::SystemUi));
            }
            FontFamily::Ui => {
                if self.has_ui {
                    list.push(named(UI_FAMILY));
                }
                list.push(FontFamilyName::Generic(GenericFamily::SystemUi));
            }
            FontFamily::Monospace => {
                list.push(FontFamilyName::Generic(GenericFamily::UiMonospace));
                list.push(FontFamilyName::Generic(GenericFamily::Monospace));
            }
            FontFamily::Named(name) => {
                list.push(FontFamilyName::Named(Cow::Owned(name.clone())));
                list.push(FontFamilyName::Generic(GenericFamily::SystemUi));
            }
        }
        list
    }
}

#[cfg(target_os = "macos")]
const ROUNDED_FONT_PATH: Option<&str> = Some("/System/Library/Fonts/SFNSRounded.ttf");
#[cfg(target_os = "macos")]
const UI_FONT_PATH: Option<&str> = Some("/System/Library/Fonts/SFNS.ttf");
#[cfg(not(target_os = "macos"))]
const ROUNDED_FONT_PATH: Option<&str> = None;
#[cfg(not(target_os = "macos"))]
const UI_FONT_PATH: Option<&str> = None;

/// Loads a font file that ships with the OS and registers it under a stable family name.
/// Returns `false` (and the generic fallbacks apply) if the file isn't there.
fn register_system_face(fonts: &mut FontContext, path: Option<&str>, family: &str) -> bool {
    let Some(path) = path else { return false };
    match std::fs::read(path) {
        Ok(bytes) => {
            let registered = fonts.collection.register_fonts(
                bytes.into(),
                Some(parley::fontique::FontInfoOverride {
                    family_name: Some(family),
                    ..Default::default()
                }),
            );
            !registered.is_empty()
        }
        Err(err) => {
            log::warn!("rugkit: couldn't load system font {path}: {err}; using fallback");
            false
        }
    }
}
