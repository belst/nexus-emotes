use gif_dispose::RGBA8;
use nexus::imgui::Image;
use nexus::imgui::TextureId;
use nexus::imgui::Ui;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::{io::Read, time::Instant};
use ureq::BodyReader;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

#[derive(Debug, Clone)]
pub struct GifFrame {
    pub id: ID3D11ShaderResourceView,
    pub delay: f32,
}

impl GifFrame {
    pub fn get_id(&self) -> TextureId {
        unsafe { std::mem::transmute::<&ID3D11ShaderResourceView, &NonNull<c_void>>(&self.id) }
            .as_ptr()
            .into()
    }
}

#[derive(Debug, Clone)]
pub struct Gif {
    pub frames: Vec<GifFrame>,
    pub height: f32,
    pub width: f32,
}

pub static TEXTURE_QUEUE: Mutex<Vec<(String, BodyReader<'static>)>> =
    const { Mutex::new(Vec::new()) };

pub fn process_queue(device: &ID3D11Device) -> anyhow::Result<Vec<(String, Gif)>> {
    TEXTURE_QUEUE
        .lock()
        .unwrap()
        .drain(..)
        .map(|(identifier, reader)| {
            let gif = load_gif(device, reader)?;
            Ok((identifier, gif))
        })
        .collect()
}

impl Gif {
    pub fn size(&self) -> [f32; 2] {
        [self.width, self.height]
    }

    pub fn load(identifier: String, url: &str) -> anyhow::Result<()> {
        let response = ureq::get(url).call()?;
        TEXTURE_QUEUE
            .lock()
            .unwrap()
            .push((identifier, response.into_body().into_reader()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct GifState {
    pub frames: Gif,
    pub current_frame: usize,
    pub timestamp: Option<Instant>,
}

impl GifState {
    pub fn new(frames: Gif) -> Self {
        Self {
            frames,
            current_frame: 0,
            timestamp: None,
        }
    }
    pub fn advance(&mut self, ui: &Ui) {
        if let Some(timestamp) = self.timestamp {
            if timestamp.elapsed().as_millis() as f32 > self.frames.frames[self.current_frame].delay
            {
                self.current_frame = (self.current_frame + 1) % self.frames.frames.len();
                self.timestamp = Some(Instant::now());
            }
        } else {
            self.timestamp = Some(Instant::now());
        }
        Image::new(
            self.frames.frames[self.current_frame].get_id(),
            self.frames.size(),
        )
        .build(ui);
    }
}

pub fn load_gif(device: &ID3D11Device, bytes: impl Read) -> anyhow::Result<Gif> {
    let mut gif_opts = gif::DecodeOptions::new();
    // Important:
    gif_opts.set_color_output(gif::ColorOutput::Indexed);

    let decoder = gif_opts.read_info(bytes)?;
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);

    let frames = decoder
        .into_iter()
        .map(|frame| {
            let frame = frame?;
            screen.blit_frame(&frame)?;
            let data = screen.pixels_rgba().to_contiguous_buf();
            let srv = create_shader_resource_view(
                device,
                unsafe { std::mem::transmute::<&[RGBA8], &[u8]>(data.0.as_ref()) },
                screen.width() as u32,
                screen.height() as u32,
            )?;
            Ok(GifFrame {
                id: srv,
                delay: 10.0 * frame.delay as f32,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Gif {
        frames,
        width: screen.width() as f32,
        height: screen.height() as f32,
    })
}

pub fn create_shader_resource_view(
    device: &ID3D11Device,
    data: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<ID3D11ShaderResourceView> {
    // Create a texture description
    let texture_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };

    // Create the texture
    let texture_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: data.as_ptr() as *const _,
        SysMemPitch: (width * 4), // 4 bytes per pixel for RGBA
        SysMemSlicePitch: 0,
    };

    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe {
        device.CreateTexture2D(&texture_desc, Some(&texture_data), Some(&mut texture))?;
    }
    let texture = texture.ok_or_else(windows::core::Error::from_win32)?;

    // Create the shader resource view
    let mut srv: Option<ID3D11ShaderResourceView> = None;
    let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_SRV {
                MostDetailedMip: 0,
                MipLevels: 1,
            },
        },
    };

    unsafe {
        device.CreateShaderResourceView(&texture, Some(&srv_desc), Some(&mut srv))?;
    }

    Ok(srv.ok_or_else(windows::core::Error::from_win32)?)
}
