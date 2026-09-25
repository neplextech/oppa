//! Narrow unsafe wrapper for Windows GDI and the installed printer driver.

#![allow(unsafe_code)]

use std::{ffi::c_void, mem::size_of, ptr};

use oppa_renderer::{PixelFormat, RasterPage};
use windows_sys::Win32::{
    Foundation::GetLastError,
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDCW, DIB_RGB_COLORS, DeleteDC, GDI_ERROR,
        GetDeviceCaps, HDC, LOGPIXELSX, LOGPIXELSY, PHYSICALOFFSETX, PHYSICALOFFSETY, RGBQUAD,
        SRCCOPY, StretchDIBits,
    },
    Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage},
};

use crate::DriverPrintApi;

const MAX_GDI_PAGE_PIXELS: u64 = 42_000_000;

pub(super) struct GdiPrintApi;

pub(super) struct GdiContext {
    hdc: HDC,
    printer_dpi_x: i32,
    printer_dpi_y: i32,
    physical_offset_x: i32,
    physical_offset_y: i32,
}

impl DriverPrintApi for GdiPrintApi {
    type Context = GdiContext;

    fn open(&mut self, queue_name: &str) -> Result<Self::Context, String> {
        let queue_name = wide(queue_name);
        let spooler = wide("WINSPOOL");
        // SAFETY: both strings are nul-terminated and remain alive through the call.
        let hdc = unsafe {
            CreateDCW(
                spooler.as_ptr(),
                queue_name.as_ptr(),
                ptr::null(),
                ptr::null(),
            )
        };
        if hdc.is_null() {
            return Err(last_error("CreateDCW"));
        }

        // SAFETY: hdc is a live device context returned by CreateDCW.
        let (printer_dpi_x, printer_dpi_y, physical_offset_x, physical_offset_y) = unsafe {
            (
                GetDeviceCaps(hdc, LOGPIXELSX as i32),
                GetDeviceCaps(hdc, LOGPIXELSY as i32),
                GetDeviceCaps(hdc, PHYSICALOFFSETX as i32),
                GetDeviceCaps(hdc, PHYSICALOFFSETY as i32),
            )
        };
        if printer_dpi_x <= 0 || printer_dpi_y <= 0 {
            // SAFETY: hdc was created above and has not been deleted.
            unsafe { DeleteDC(hdc) };
            return Err("printer driver reported invalid device resolution".to_owned());
        }
        Ok(GdiContext {
            hdc,
            printer_dpi_x,
            printer_dpi_y,
            physical_offset_x,
            physical_offset_y,
        })
    }

    fn start_document(&mut self, context: &mut Self::Context) -> Result<u32, String> {
        let document_name = wide("OPPA print job");
        let info = DOCINFOW {
            cbSize: i32::try_from(size_of::<DOCINFOW>())
                .map_err(|_| "DOCINFOW size exceeds i32".to_owned())?,
            lpszDocName: document_name.as_ptr(),
            lpszOutput: ptr::null(),
            lpszDatatype: ptr::null(),
            fwType: 0,
        };
        // SAFETY: context owns an open printer HDC; info's pointer remains valid.
        let job_id = unsafe { StartDocW(context.hdc, &raw const info) };
        if job_id <= 0 {
            Err(last_error("StartDocW"))
        } else {
            u32::try_from(job_id).map_err(|_| "driver returned a negative job ID".to_owned())
        }
    }

    fn start_page(&mut self, context: &mut Self::Context) -> Result<(), String> {
        // SAFETY: document was started on this live HDC.
        if unsafe { StartPage(context.hdc) } <= 0 {
            Err(last_error("StartPage"))
        } else {
            Ok(())
        }
    }

    fn draw_page(&mut self, context: &mut Self::Context, page: &RasterPage) -> Result<(), String> {
        if u64::from(page.width).saturating_mul(u64::from(page.height)) > MAX_GDI_PAGE_PIXELS {
            return Err("raster page exceeds the GDI pixel limit".to_owned());
        }
        let bits = make_top_down_dib(page)?;
        let width = i32::try_from(page.width).map_err(|_| "page width exceeds GDI limits")?;
        let height = i32::try_from(page.height).map_err(|_| "page height exceeds GDI limits")?;
        let destination_width = scale_to_device(page.width, context.printer_dpi_x, page.dpi_x)?;
        let destination_height = scale_to_device(page.height, context.printer_dpi_y, page.dpi_y)?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>())
                    .map_err(|_| "bitmap header size exceeds u32")?,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 1,
                biCompression: BI_RGB,
                biSizeImage: u32::try_from(bits.len()).map_err(|_| "DIB payload exceeds u32")?,
                ..BITMAPINFOHEADER::default()
            },
            // For one-bit DIBs, pixel bit 0 maps to white and bit 1 to black.
            bmiColors: [RGBQUAD {
                rgbBlue: 255,
                rgbGreen: 255,
                rgbRed: 255,
                rgbReserved: 0,
            }],
        };
        // BITMAPINFO declares one palette entry; allocate room for its second
        // 1bpp entry, which follows the struct in the DIB memory layout.
        let mut info_storage = [0_u64; 6];
        let dib_info = info_storage.as_mut_ptr().cast::<BITMAPINFO>();
        // SAFETY: info_storage is 48 bytes, enough for BITMAPINFO plus a second
        // RGBQUAD; the storage is aligned for BITMAPINFO.
        unsafe {
            dib_info.write(info);
            dib_info
                .cast::<u8>()
                .add(size_of::<BITMAPINFO>())
                .cast::<RGBQUAD>()
                .write(RGBQUAD {
                    rgbBlue: 0,
                    rgbGreen: 0,
                    rgbRed: 0,
                    rgbReserved: 0,
                });
        }
        // SAFETY: DIB data, the extended two-color BITMAPINFO storage, and the
        // printer HDC stay alive for the duration of StretchDIBits.
        let copied = unsafe {
            StretchDIBits(
                context.hdc,
                -context.physical_offset_x,
                -context.physical_offset_y,
                destination_width,
                destination_height,
                0,
                0,
                width,
                height,
                bits.as_ptr().cast::<c_void>(),
                dib_info.cast_const(),
                DIB_RGB_COLORS,
                SRCCOPY,
            )
        };
        if copied == GDI_ERROR || copied == 0 {
            Err(last_error("StretchDIBits"))
        } else {
            Ok(())
        }
    }

    fn end_page(&mut self, context: &mut Self::Context) -> Result<(), String> {
        // SAFETY: the active page belongs to this live HDC.
        if unsafe { EndPage(context.hdc) } <= 0 {
            Err(last_error("EndPage"))
        } else {
            Ok(())
        }
    }

    fn end_document(&mut self, context: &mut Self::Context) -> Result<(), String> {
        // SAFETY: the print document belongs to this live HDC.
        if unsafe { EndDoc(context.hdc) } <= 0 {
            Err(last_error("EndDoc"))
        } else {
            Ok(())
        }
    }

    fn abort_document(&mut self, context: &mut Self::Context) {
        // SAFETY: cancellation is attempted only after a successful StartDocW.
        unsafe { AbortDoc(context.hdc) };
    }

    fn close(&mut self, context: Self::Context) {
        // SAFETY: this context exclusively owns the HDC.
        unsafe { DeleteDC(context.hdc) };
    }
}

fn make_top_down_dib(page: &RasterPage) -> Result<Vec<u8>, String> {
    let source_stride =
        usize::try_from(page.width.div_ceil(8)).map_err(|_| "page source stride exceeds usize")?;
    let dib_stride = usize::try_from(page.width.div_ceil(32))
        .map_err(|_| "page DIB stride exceeds usize")?
        .checked_mul(4)
        .ok_or_else(|| "page DIB stride overflow".to_owned())?;
    let height = usize::try_from(page.height).map_err(|_| "page height exceeds usize")?;
    let size = dib_stride
        .checked_mul(height)
        .ok_or_else(|| "DIB payload size overflow".to_owned())?;
    let width = usize::try_from(page.width).map_err(|_| "page width exceeds usize")?;
    let mut output = vec![0; size];
    for y in 0..height {
        for x in 0..width {
            let black = match page.format {
                PixelFormat::Mono1 => {
                    let index = y * source_stride + x / 8;
                    page.data[index] & (0x80 >> (x % 8)) != 0
                }
                PixelFormat::Gray8 => page.data[y * width + x] < 128,
                PixelFormat::Rgb24 | PixelFormat::Rgba32 => {
                    let channels = if page.format == PixelFormat::Rgb24 {
                        3
                    } else {
                        4
                    };
                    let index = (y * width + x) * channels;
                    let red = u16::from(page.data[index]);
                    let green = u16::from(page.data[index + 1]);
                    let blue = u16::from(page.data[index + 2]);
                    let alpha = if channels == 4 {
                        u16::from(page.data[index + 3])
                    } else {
                        255
                    };
                    let luminance = (red * 299 + green * 587 + blue * 114) / 1_000;
                    let composited = (luminance * alpha + 255 * (255 - alpha)) / 255;
                    composited < 128
                }
            };
            if black {
                let index = y * dib_stride + x / 8;
                output[index] |= 0x80 >> (x % 8);
            }
        }
    }
    Ok(output)
}

fn scale_to_device(pixels: u32, printer_dpi: i32, raster_dpi: u16) -> Result<i32, String> {
    let scaled = u64::from(pixels)
        .checked_mul(u64::try_from(printer_dpi).map_err(|_| "invalid device resolution")?)
        .and_then(|value| value.checked_add(u64::from(raster_dpi / 2)))
        .map(|value| value / u64::from(raster_dpi));
    i32::try_from(scaled.ok_or_else(|| "destination dimensions overflow".to_owned())?)
        .map_err(|_| "destination dimensions exceed GDI limits".to_owned())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(operation: &str) -> String {
    // SAFETY: GetLastError has no preconditions and returns thread-local error state.
    let code = unsafe { GetLastError() };
    if code == 0 {
        format!("{operation} failed")
    } else {
        format!("{operation} failed with Windows error {code}")
    }
}
