//! Terminal-friendly QR rendering using the `qrcode` crate's
//! `unicode::Dense1x2` glyph set (two pixel rows per character cell).

use anyhow::Result;
use qrcode::QrCode;
use qrcode::render::unicode;

pub fn render(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes())?;
    let image = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    Ok(image)
}
