//! Safe interpreter for the ESC/POS subset emitted by OPPA.

use oppa_protocol::{BarcodeFormat, ReceiptWidth, TextAlignment};

use crate::{PrintLayout, ReceiptElement, ReceiptImage, RendererError, RendererResult};

const ESC: u8 = 0x1b;
const GS: u8 = 0x1d;
const MAX_RASTER_BYTES: usize = 2 * 1024 * 1024;
const MAX_QR_BYTES: usize = 4_096;
const MAX_TEXT_BYTES: usize = 256 * 1024;

/// A command the supported interpreter subset cannot reproduce safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedEscPosCommand {
    /// Offset at which the command starts.
    pub offset: usize,
    /// Short command description without payload bytes.
    pub command: String,
}

/// Semantic diagnostics collected while interpreting an ESC/POS stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EscPosDiagnostics {
    /// Number of control commands parsed before the stream ended or stopped.
    pub interpreted_commands: usize,
    /// Number of text lines reconstructed from the byte stream.
    pub text_lines: usize,
    /// Number of raster image blocks.
    pub images: usize,
    /// Number of QR code print operations.
    pub qr_codes: usize,
    /// Number of barcode print operations.
    pub barcodes: usize,
    /// Total explicit feed lines.
    pub feed_lines: usize,
    /// Whether one or more paper cuts were requested.
    pub cut_requested: bool,
    /// Unsupported commands that caused parsing to stop.
    pub unsupported_commands: Vec<UnsupportedEscPosCommand>,
}

/// Result of interpreting the supported ESC/POS subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscPosInterpretation {
    /// Common receipt layout represented by the command stream.
    pub layout: PrintLayout,
    /// Bounded summary of commands and device-specific behavior.
    pub diagnostics: EscPosDiagnostics,
}

/// Interprets the ESC/POS subset currently emitted by OPPA's renderer.
#[derive(Debug, Clone, Copy)]
pub struct EscPosInterpreter {
    receipt_width: ReceiptWidth,
}

/// Encodes the shared receipt layout as an ESC/POS stream.
pub(crate) fn render_layout(layout: &PrintLayout) -> RendererResult<Vec<u8>> {
    let columns = super::columns(layout.width);
    let mut output = vec![ESC, b'@'];
    for element in &layout.elements {
        match element {
            ReceiptElement::Text {
                value,
                alignment,
                bold,
            } => {
                super::require_ascii(value)?;
                output.extend_from_slice(&[
                    ESC,
                    b'a',
                    super::alignment_code(Some(*alignment)),
                    ESC,
                    b'E',
                    u8::from(*bold),
                ]);
                for line in super::wrap_text(value, columns) {
                    output.extend_from_slice(line.as_bytes());
                    output.push(b'\n');
                }
                output.extend_from_slice(&[ESC, b'E', 0]);
            }
            ReceiptElement::Row { left, right } => {
                super::require_ascii(left)?;
                super::require_ascii(right)?;
                output.extend_from_slice(&[ESC, b'a', 0]);
                for line in super::layout_row(left, right, columns) {
                    output.extend_from_slice(line.as_bytes());
                    output.push(b'\n');
                }
            }
            ReceiptElement::Divider => {
                output.extend(std::iter::repeat_n(b'-', columns));
                output.push(b'\n');
            }
            ReceiptElement::Image(image) => {
                let row_bytes = image.stride;
                let width_low = u8::try_from(row_bytes & 0xff).map_err(|_| {
                    RendererError::InvalidRasterPage("image width overflow".to_owned())
                })?;
                let width_high = u8::try_from((row_bytes >> 8) & 0xff).map_err(|_| {
                    RendererError::InvalidRasterPage("image width overflow".to_owned())
                })?;
                let height_low = u8::try_from(image.height & 0xff).map_err(|_| {
                    RendererError::InvalidRasterPage("image height overflow".to_owned())
                })?;
                let height_high = u8::try_from((image.height >> 8) & 0xff).map_err(|_| {
                    RendererError::InvalidRasterPage("image height overflow".to_owned())
                })?;
                output.extend_from_slice(&[
                    GS,
                    b'v',
                    b'0',
                    0,
                    width_low,
                    width_high,
                    height_low,
                    height_high,
                ]);
                output.extend_from_slice(&image.data);
            }
            ReceiptElement::Qr(value) => output.extend(super::render_qr(value)),
            ReceiptElement::Barcode {
                format,
                value,
                height_dots,
                human_readable,
            } => {
                let mut bytes = super::render_barcode(*format, value)?;
                bytes[2] = if *human_readable { 2 } else { 0 };
                bytes[5] = *height_dots;
                output.extend(bytes);
            }
            ReceiptElement::Feed { lines } => output.extend_from_slice(&[ESC, b'd', *lines]),
            ReceiptElement::Cut => output.extend_from_slice(&[GS, b'V', 0]),
        }
    }
    Ok(output)
}

impl EscPosInterpreter {
    /// Creates an interpreter with the physical receipt width expected by the
    /// target device. ESC/POS does not carry a standardized width declaration.
    pub const fn new(receipt_width: ReceiptWidth) -> Self {
        Self { receipt_width }
    }

    /// Converts OPPA-generated ESC/POS bytes into the common print layout.
    ///
    /// Unknown commands stop interpretation immediately. Their following bytes
    /// are never treated as printable text, so parser desynchronization cannot
    /// turn arbitrary control data into office-page content.
    pub fn interpret(self, bytes: &[u8]) -> RendererResult<EscPosInterpretation> {
        let mut parser = Parser::new(bytes, self.receipt_width);
        parser.run()
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    layout: PrintLayout,
    diagnostics: EscPosDiagnostics,
    alignment: TextAlignment,
    bold: bool,
    text: Vec<u8>,
    qr_payload: Option<String>,
    barcode_height_dots: u8,
    barcode_hri: bool,
    stopped: bool,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8], receipt_width: ReceiptWidth) -> Self {
        Self {
            bytes,
            offset: 0,
            layout: PrintLayout {
                width: receipt_width,
                elements: Vec::new(),
            },
            diagnostics: EscPosDiagnostics::default(),
            alignment: TextAlignment::Left,
            bold: false,
            text: Vec::new(),
            qr_payload: None,
            barcode_height_dots: 80,
            barcode_hri: true,
            stopped: false,
        }
    }

    fn run(&mut self) -> RendererResult<EscPosInterpretation> {
        while self.offset < self.bytes.len() && !self.stopped {
            let command_offset = self.offset;
            match self.bytes[self.offset] {
                ESC => self.parse_escape(command_offset)?,
                GS => self.parse_group_separator(command_offset)?,
                b'\n' => {
                    self.offset += 1;
                    self.finish_text_line()?;
                }
                b'\r' => self.offset += 1,
                byte if byte.is_ascii_graphic() || byte == b' ' => {
                    self.text.push(byte);
                    self.offset += 1;
                    if self.text.len() > MAX_TEXT_BYTES {
                        return self.invalid(command_offset, "text segment exceeds its limit");
                    }
                }
                byte => {
                    self.unsupported(
                        command_offset,
                        format!("non-printable byte 0x{byte:02X} outside a supported command"),
                    );
                }
            }
        }
        if !self.stopped {
            self.flush_text(false)?;
        }
        Ok(EscPosInterpretation {
            layout: self.layout.clone(),
            diagnostics: self.diagnostics.clone(),
        })
    }

    fn parse_escape(&mut self, command_offset: usize) -> RendererResult<()> {
        self.flush_text(false)?;
        let Some(command) = self.bytes.get(self.offset + 1).copied() else {
            return self.invalid(command_offset, "truncated ESC command");
        };
        self.offset += 2;
        self.diagnostics.interpreted_commands += 1;
        match command {
            b'@' => {
                self.alignment = TextAlignment::Left;
                self.bold = false;
                self.qr_payload = None;
            }
            b'a' => {
                let Some(value) = self.take_byte(command_offset, "alignment value")? else {
                    return Ok(());
                };
                self.alignment = match value {
                    0 => TextAlignment::Left,
                    1 => TextAlignment::Center,
                    2 => TextAlignment::Right,
                    _ => {
                        self.unsupported(command_offset, format!("ESC a alignment {value}"));
                        return Ok(());
                    }
                };
            }
            b'E' => {
                let Some(value) = self.take_byte(command_offset, "bold value")? else {
                    return Ok(());
                };
                match value {
                    0 => self.bold = false,
                    1 => self.bold = true,
                    _ => self.unsupported(command_offset, format!("ESC E value {value}")),
                }
            }
            b'd' => {
                let Some(lines) = self.take_byte(command_offset, "feed line count")? else {
                    return Ok(());
                };
                if lines == 0 {
                    self.unsupported(command_offset, "ESC d feed 0".to_owned());
                } else {
                    self.layout.elements.push(ReceiptElement::Feed { lines });
                    self.diagnostics.feed_lines += usize::from(lines);
                }
            }
            other => self.unsupported(command_offset, format!("ESC 0x{other:02X}")),
        }
        Ok(())
    }

    fn parse_group_separator(&mut self, command_offset: usize) -> RendererResult<()> {
        self.flush_text(false)?;
        let Some(command) = self.bytes.get(self.offset + 1).copied() else {
            return self.invalid(command_offset, "truncated GS command");
        };
        self.offset += 2;
        self.diagnostics.interpreted_commands += 1;
        match command {
            b'v' => self.parse_raster_image(command_offset)?,
            b'(' => self.parse_qr_command(command_offset)?,
            b'H' => {
                let Some(value) = self.take_byte(command_offset, "barcode text mode")? else {
                    return Ok(());
                };
                if value > 3 {
                    self.unsupported(command_offset, format!("GS H text mode {value}"));
                } else {
                    self.barcode_hri = value != 0;
                }
            }
            b'h' => {
                let Some(value) = self.take_byte(command_offset, "barcode height")? else {
                    return Ok(());
                };
                self.barcode_height_dots = value;
            }
            b'k' => self.parse_barcode(command_offset)?,
            b'V' => {
                let Some(mode) = self.take_byte(command_offset, "cut mode")? else {
                    return Ok(());
                };
                if mode > 1 {
                    self.unsupported(command_offset, format!("GS V cut mode {mode}"));
                } else {
                    self.layout.elements.push(ReceiptElement::Cut);
                    self.diagnostics.cut_requested = true;
                }
            }
            other => self.unsupported(command_offset, format!("GS 0x{other:02X}")),
        }
        Ok(())
    }

    fn parse_raster_image(&mut self, command_offset: usize) -> RendererResult<()> {
        let Some(subcommand) = self.take_byte(command_offset, "raster subcommand")? else {
            return Ok(());
        };
        if subcommand != b'0' {
            self.unsupported(command_offset, format!("GS v mode 0x{subcommand:02X}"));
            return Ok(());
        }
        let Some(mode) = self.take_byte(command_offset, "raster mode")? else {
            return Ok(());
        };
        if mode != 0 {
            self.unsupported(command_offset, format!("GS v 0 mode {mode}"));
            return Ok(());
        }
        let Some(width_low) = self.take_byte(command_offset, "raster row width")? else {
            return Ok(());
        };
        let Some(width_high) = self.take_byte(command_offset, "raster row width")? else {
            return Ok(());
        };
        let Some(height_low) = self.take_byte(command_offset, "raster height")? else {
            return Ok(());
        };
        let Some(height_high) = self.take_byte(command_offset, "raster height")? else {
            return Ok(());
        };
        let row_bytes = u32::from(width_low) | (u32::from(width_high) << 8);
        let height = u32::from(height_low) | (u32::from(height_high) << 8);
        let Some(byte_count) = usize::try_from(row_bytes).ok().and_then(|row| {
            usize::try_from(height)
                .ok()
                .and_then(|rows| row.checked_mul(rows))
        }) else {
            return self.invalid(command_offset, "raster dimensions overflow");
        };
        if row_bytes == 0 || height == 0 || byte_count > MAX_RASTER_BYTES {
            return self.invalid(
                command_offset,
                "raster block exceeds its supported dimensions",
            );
        }
        let Some(payload) = self.take_payload(byte_count, command_offset, "raster pixels")? else {
            return Ok(());
        };
        let image = ReceiptImage::from_mono(row_bytes * 8, height, payload.to_vec())?;
        self.layout.elements.push(ReceiptElement::Image(image));
        self.diagnostics.images += 1;
        Ok(())
    }

    fn parse_qr_command(&mut self, command_offset: usize) -> RendererResult<()> {
        let Some(function) = self.take_byte(command_offset, "GS ( function")? else {
            return Ok(());
        };
        if function != b'k' {
            self.unsupported(command_offset, format!("GS ( 0x{function:02X}"));
            return Ok(());
        }
        let Some(length_low) = self.take_byte(command_offset, "QR payload length")? else {
            return Ok(());
        };
        let Some(length_high) = self.take_byte(command_offset, "QR payload length")? else {
            return Ok(());
        };
        let length = usize::from(length_low) | (usize::from(length_high) << 8);
        if length == 0 || length > MAX_QR_BYTES + 3 {
            return self.invalid(command_offset, "QR payload length exceeds its limit");
        }
        let Some(payload) = self.take_payload(length, command_offset, "QR payload")? else {
            return Ok(());
        };
        if payload.len() < 3 || payload[0] != 49 {
            self.unsupported(
                command_offset,
                "QR command has an unknown parameter prefix".to_owned(),
            );
            return Ok(());
        }
        match payload[1] {
            65 if payload.get(2) == Some(&50) => {}
            67 if payload.len() == 3 && (1..=16).contains(&payload[2]) => {}
            69 if payload.len() == 3 && (48..=51).contains(&payload[2]) => {}
            80 if payload.len() >= 3 && payload[2] == 48 => {
                let value = std::str::from_utf8(&payload[3..]).map_err(|_| {
                    RendererError::InvalidEscPos {
                        offset: command_offset,
                        reason: "QR payload is not UTF-8".to_owned(),
                    }
                })?;
                self.qr_payload = Some(value.to_owned());
            }
            81 if payload.len() == 3 && payload[2] == 48 => {
                if let Some(value) = self.qr_payload.take() {
                    self.layout.elements.push(ReceiptElement::Qr(value));
                    self.diagnostics.qr_codes += 1;
                } else {
                    self.unsupported(command_offset, "QR print without stored data".to_owned());
                }
            }
            command => self.unsupported(command_offset, format!("QR function {command}")),
        }
        Ok(())
    }

    fn parse_barcode(&mut self, command_offset: usize) -> RendererResult<()> {
        let Some(command) = self.take_byte(command_offset, "barcode type")? else {
            return Ok(());
        };
        let Some(length) = self.take_byte(command_offset, "barcode length")? else {
            return Ok(());
        };
        let Some(payload) =
            self.take_payload(usize::from(length), command_offset, "barcode data")?
        else {
            return Ok(());
        };
        let (format, value) = match command {
            73 => {
                if !payload.starts_with(b"{B") {
                    self.unsupported(
                        command_offset,
                        "Code 128 command must use the emitted set-B subset".to_owned(),
                    );
                    return Ok(());
                }
                (BarcodeFormat::Code128, &payload[2..])
            }
            69 => (BarcodeFormat::Code39, payload),
            67 => (BarcodeFormat::Ean13, payload),
            65 => (BarcodeFormat::Upca, payload),
            other => {
                self.unsupported(command_offset, format!("barcode type {other}"));
                return Ok(());
            }
        };
        let value = std::str::from_utf8(value).map_err(|_| RendererError::InvalidEscPos {
            offset: command_offset,
            reason: "barcode data is not ASCII".to_owned(),
        })?;
        self.layout.elements.push(ReceiptElement::Barcode {
            format,
            value: value.to_owned(),
            height_dots: self.barcode_height_dots,
            human_readable: self.barcode_hri,
        });
        self.diagnostics.barcodes += 1;
        Ok(())
    }

    fn finish_text_line(&mut self) -> RendererResult<()> {
        if self.text.is_empty() {
            self.layout.elements.push(ReceiptElement::Feed { lines: 1 });
            self.diagnostics.feed_lines += 1;
            return Ok(());
        }
        let text = String::from_utf8(std::mem::take(&mut self.text)).map_err(|_| {
            RendererError::InvalidEscPos {
                offset: self.offset,
                reason: "text bytes are not ASCII".to_owned(),
            }
        })?;
        self.diagnostics.text_lines += 1;
        if text.len() == super::columns(self.layout.width) && text.bytes().all(|byte| byte == b'-')
        {
            self.layout.elements.push(ReceiptElement::Divider);
        } else if let Some((left, right)) = split_emitted_row(&text) {
            self.layout
                .elements
                .push(ReceiptElement::Row { left, right });
        } else {
            self.layout.elements.push(ReceiptElement::Text {
                value: text,
                alignment: self.alignment,
                bold: self.bold,
            });
        }
        Ok(())
    }

    fn flush_text(&mut self, count_line: bool) -> RendererResult<()> {
        if self.text.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8(std::mem::take(&mut self.text)).map_err(|_| {
            RendererError::InvalidEscPos {
                offset: self.offset,
                reason: "text bytes are not ASCII".to_owned(),
            }
        })?;
        self.layout.elements.push(ReceiptElement::Text {
            value: text,
            alignment: self.alignment,
            bold: self.bold,
        });
        if count_line {
            self.diagnostics.text_lines += 1;
        }
        Ok(())
    }

    fn take_byte(
        &mut self,
        command_offset: usize,
        description: &'static str,
    ) -> RendererResult<Option<u8>> {
        let Some(value) = self.bytes.get(self.offset).copied() else {
            self.invalid(command_offset, format!("truncated {description}"))?;
            return Ok(None);
        };
        self.offset += 1;
        Ok(Some(value))
    }

    fn take_payload(
        &mut self,
        length: usize,
        command_offset: usize,
        description: &'static str,
    ) -> RendererResult<Option<&'a [u8]>> {
        let Some(end) = self.offset.checked_add(length) else {
            self.invalid(command_offset, format!("{description} length overflow"))?;
            return Ok(None);
        };
        let Some(payload) = self.bytes.get(self.offset..end) else {
            self.invalid(command_offset, format!("truncated {description}"))?;
            return Ok(None);
        };
        self.offset = end;
        Ok(Some(payload))
    }

    fn unsupported(&mut self, offset: usize, command: String) {
        self.diagnostics
            .unsupported_commands
            .push(UnsupportedEscPosCommand { offset, command });
        self.stopped = true;
    }

    fn invalid<T>(&self, offset: usize, reason: impl Into<String>) -> RendererResult<T> {
        Err(RendererError::InvalidEscPos {
            offset,
            reason: reason.into(),
        })
    }
}

fn split_emitted_row(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    let mut selected = None;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b' ' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] == b' ' {
            index += 1;
        }
        if index - start >= 2 {
            selected = Some((start, index));
        }
    }
    let (start, end) = selected?;
    let left = line[..start].trim_end();
    let right = line[end..].trim_start();
    if left.is_empty() {
        return None;
    }
    Some((left.to_owned(), right.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use image::ImageEncoder;
    use oppa_protocol::{PrintDocument, PrintSection, ReceiptWidth, TextAlignment};

    use crate::{DocumentRenderer, EscPosInterpreter, RenderTarget, RendererError};

    #[test]
    fn interprets_every_command_emitted_for_structured_receipts() {
        let image = image::GrayImage::from_fn(8, 8, |x, y| {
            if x == y || x + y == 7 {
                image::Luma([0])
            } else {
                image::Luma([255])
            }
        });
        let mut encoded_image = Cursor::new(Vec::new());
        image::codecs::png::PngEncoder::new(&mut encoded_image)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::L8,
            )
            .expect("encode fixture image");
        let document = PrintDocument {
            width: ReceiptWidth::Mm80,
            sections: vec![
                PrintSection::Text {
                    value: "Title".to_owned(),
                    align: Some(TextAlignment::Center),
                    bold: Some(true),
                },
                PrintSection::Row {
                    left: "Coffee".to_owned(),
                    right: "10.00".to_owned(),
                },
                PrintSection::Divider,
                PrintSection::Image {
                    media_type: oppa_protocol::ImageMediaType::Png,
                    data: STANDARD.encode(encoded_image.into_inner()),
                },
                PrintSection::Qr {
                    value: "https://openprinter.dev".to_owned(),
                },
                PrintSection::Barcode {
                    format: oppa_protocol::BarcodeFormat::Code128,
                    value: "RECEIPT-42".to_owned(),
                },
                PrintSection::Feed { lines: 2 },
                PrintSection::Cut,
            ],
        };
        let rendered = DocumentRenderer::default()
            .render(&document, RenderTarget::EscPos)
            .expect("render escpos");
        let crate::RenderedDocument::EscPos(document) = rendered else {
            panic!("expected ESC/POS bytes");
        };
        let decoded = EscPosInterpreter::new(ReceiptWidth::Mm80)
            .interpret(&document.bytes)
            .expect("interpret emitted subset");
        assert_eq!(decoded.diagnostics.qr_codes, 1);
        assert_eq!(decoded.diagnostics.images, 1);
        assert_eq!(decoded.diagnostics.barcodes, 1);
        assert_eq!(decoded.diagnostics.feed_lines, 2);
        assert!(decoded.diagnostics.cut_requested);
        assert!(decoded.diagnostics.unsupported_commands.is_empty());
        assert!(decoded.layout.elements.iter().any(|element| matches!(
            element,
            crate::ReceiptElement::Text { value, alignment: TextAlignment::Center, bold: true }
                if value == "Title"
        )));
        assert!(decoded.layout.elements.iter().any(|element| matches!(
            element,
            crate::ReceiptElement::Row { left, right }
                if left == "Coffee" && right == "10.00"
        )));
        assert!(
            decoded
                .layout
                .elements
                .contains(&crate::ReceiptElement::Divider)
        );
        assert!(
            decoded
                .layout
                .elements
                .contains(&crate::ReceiptElement::Feed { lines: 2 })
        );
        assert!(
            decoded
                .layout
                .elements
                .contains(&crate::ReceiptElement::Cut)
        );
    }

    #[test]
    fn unknown_commands_stop_without_interpreting_trailing_bytes_as_text() {
        let bytes = [0x1b, b'@', 0x1b, b'X', b'G', b'A', b'R', b'B'];
        let decoded = EscPosInterpreter::new(ReceiptWidth::Mm58)
            .interpret(&bytes)
            .expect("unsupported command is recorded");
        assert_eq!(decoded.diagnostics.unsupported_commands.len(), 1);
        assert!(decoded.layout.elements.is_empty());
        assert!(matches!(
            crate::render_escpos_for_page(
                &bytes,
                ReceiptWidth::Mm58,
                crate::PageRenderOptions::A4_PORTRAIT
            ),
            Err(RendererError::InvalidEscPos { offset: 2, .. })
        ));
    }

    #[test]
    fn truncated_commands_are_errors() {
        assert!(matches!(
            EscPosInterpreter::new(ReceiptWidth::Mm80).interpret(&[0x1b, b'a']),
            Err(RendererError::InvalidEscPos { .. })
        ));
    }
}
