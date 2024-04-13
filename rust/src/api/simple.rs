use anyhow::anyhow;
use anyhow::bail;
use anyhow::Ok;
use vello::block_on_wgpu;
use core::num::NonZeroUsize;
use vello::kurbo::Affine;
use vello::kurbo::Circle;
use vello::peniko::Color;
use vello::util::RenderContext;
use vello::AaConfig;
use vello::Renderer;
use vello::RendererOptions;
use wgpu::BufferDescriptor;
use wgpu::BufferUsages;
use wgpu::CommandEncoderDescriptor;
use wgpu::Extent3d;
use wgpu::ImageCopyBuffer;
use wgpu::TextureDescriptor;
use wgpu::TextureFormat;
use wgpu::TextureUsages;

#[flutter_rust_bridge::frb(sync)] // Synchronous mode for simplicity of the demo
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();
}

pub async fn test_render() -> Vec<u8> {
    render().await.unwrap()
}

async fn render() -> Result<Vec<u8>, anyhow::Error> {
    let (width, height) = (800, 600);

    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut context = RenderContext::new().unwrap();
    let device_id = context
        .device(None)
        .await
        .ok_or_else(|| anyhow!("No compatible device found"))?;
    let device_handle = &mut context.devices[device_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    // let texture_format: wgpu::TextureFormat = ...;
    let mut renderer = Renderer::new(
        &device,
        RendererOptions {
            //   surface_format: Some(texture_format),
            surface_format: None,
            use_cpu: false,
            antialiasing_support: vello::AaSupport::all(),
            num_init_threads: NonZeroUsize::new(1),
        },
    )
    .expect("Failed to create renderer");

    // Create scene and draw stuff in it
    let mut scene = vello::Scene::new();
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        Color::rgb8(242, 140, 168),
        None,
        &Circle::new((420.0, 200.0), 120.0),
    );

    let target = device.create_texture(&TextureDescriptor {
        label: Some("Target texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    renderer
        .render_to_texture(
            &device,
            &queue,
            &scene,
            &view,
            &vello::RenderParams {
                base_color: Color::BLACK, // Background color
                width,
                height,
                antialiasing_method: AaConfig::Msaa16,
            },
        )
        .expect("Failed to render to surface");

    renderer
        .render_to_texture(
            device,
            queue,
            &scene,
            &view,
            &vello::RenderParams {
                base_color: Color::BLACK, // Background color
                width,
                height,
                antialiasing_method: AaConfig::Msaa16,
            },
        )
        .or_else(|_| bail!("Got non-Send/Sync error from rendering"))?;
    let padded_byte_width = (width * 4).next_multiple_of(256);
    let buffer_size = padded_byte_width as u64 * height as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("val"),
        size: buffer_size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Copy out buffer"),
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_byte_width),
                rows_per_image: None,
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let buf_slice = buffer.slice(..);

    let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
    buf_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
    if let Some(recv_result) = block_on_wgpu(device, receiver.receive()) {
        recv_result?;
    } else {
        bail!("channel was closed");
    }

    let data = buf_slice.get_mapped_range();
    let mut result_unpadded = Vec::<u8>::with_capacity((width * height * 4).try_into()?);
    for row in 0..height {
        let start = (row * padded_byte_width).try_into()?;
        result_unpadded.extend(&data[start..start + (width * 4) as usize]);
    }

    Ok(result_unpadded)
}
