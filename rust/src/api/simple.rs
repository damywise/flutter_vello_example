use core::num::NonZeroUsize;
use pollster::FutureExt;
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::sync::RwLock;
use std::thread::spawn;
use vello::block_on_wgpu;
use vello::kurbo::Affine;
use vello::kurbo::Circle;
use vello::kurbo::Ellipse;
use vello::kurbo::Line;
use vello::kurbo::RoundedRect;
use vello::kurbo::Stroke;
use vello::peniko::Color;
use vello::util::RenderContext;
use vello::AaConfig;
use vello::Renderer;
use vello::RendererOptions;
use vello::Scene;
use wgpu::BufferDescriptor;
use wgpu::BufferUsages;
use wgpu::CommandEncoderDescriptor;
use wgpu::Extent3d;
use wgpu::ImageCopyBuffer;
use wgpu::TextureDescriptor;
use wgpu::TextureFormat;
use wgpu::TextureUsages;

static mut INITIALIZED: bool = false;

lazy_static::lazy_static!(
    static ref RENDERERS: RwLock<HashMap<String, SimpleContext>> = RwLock::new(HashMap::new());
);

#[flutter_rust_bridge::frb(sync)] // Synchronous mode for simplicity of the demo
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    unsafe {
        if INITIALIZED {
            return;
        }
        INITIALIZED = true;
    }

    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();

    init_renderer();
}

fn init_renderer() {
    let key: String = "key".to_string();

    {
        let map = RENDERERS.read().unwrap();
        if map.contains_key(&key) {
            let decoder = &map[&key];
            return;
        }
    }

    let (renderer_request_tx, renderer_request_rx): (
        Sender<RenderRequest>,
        Receiver<RenderRequest>,
    ) = mpsc::channel();
    let (renderer_response_tx, renderer_response_rx): (
        Sender<RenderResponse>,
        Receiver<RenderResponse>,
    ) = mpsc::channel();
    let (renderer_info_tx, renderer_info_rx): (Sender<RenderInfo>, Receiver<RenderInfo>) =
        mpsc::channel();

    let mut render_cx = RenderContext::new().unwrap();

    // let mut renderers: Vec<Option<Renderer>> = vec![];

    // renderers[0]
    //     .get_or_insert_with(|| renderer);

    let mut scene = Scene::new();

    spawn(move || {
        let device_id = render_cx.device(None).block_on().unwrap();
        let device_handle = &mut render_cx.devices[device_id];
        let device = &device_handle.device;
        let queue = &device_handle.queue;
        let renderer = Mutex::new(
            Renderer::new(
                device,
                RendererOptions {
                    surface_format: None,
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::all(),
                    num_init_threads: NonZeroUsize::new(1),
                },
            )
            .expect("Couldn't create renderer"),
        );

        let (width, height) = (800, 600);

        loop {
            let request = renderer_request_rx.recv().unwrap();
            let response = match request.command {
                RenderCommand::Render(pos) => {
                    scene.reset();
                    // add_shapes_to_scene(&mut scene);
                    fn render_rectangle_pos(scene: &mut Scene, pos: &Pos) {
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            Affine::IDENTITY,
                            Color::rgb(0.9804, 0.702, 0.5294),
                            None,
                            &RoundedRect::new(pos.x, pos.y, 0.0, 0.0, 20.0),
                        );
                    }
                    render_rectangle_pos(&mut scene, &pos);
                    let size = Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    };

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
                        .lock()
                        .unwrap()
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
                        .unwrap();

                    fn get_bytes_from_texture(
                        target: &wgpu::Texture,
                        device: &wgpu::Device,
                        queue: &wgpu::Queue,
                        size: Extent3d,
                        width: u32,
                        height: u32,
                    ) -> Vec<u8> {
                        let padded_byte_width = (width * 4 + 255) & !255; // Ensure padded_byte_width is a multiple of 256
                        let buffer_size = padded_byte_width as u64 * height as u64;
                        let buffer = device.create_buffer(&BufferDescriptor {
                            label: Some("val"),
                            size: buffer_size,
                            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        let mut encoder =
                            device.create_command_encoder(&CommandEncoderDescriptor {
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

                        let (sender, receiver) =
                            futures_intrusive::channel::shared::oneshot_channel();
                        buf_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
                        if let Some(recv_result) = block_on_wgpu(device, receiver.receive()) {
                            recv_result.expect("Failed to receive buffer data");
                        }

                        let data = buf_slice.get_mapped_range();
                        let mut result_unpadded =
                            Vec::<u8>::with_capacity((width * height * 4).try_into().unwrap());
                        for row in 0..height {
                            let start = (row * padded_byte_width).try_into().unwrap();
                            result_unpadded.extend(&data[start..start + (width * 4) as usize]);
                        }

                        result_unpadded
                    }

                    let data = get_bytes_from_texture(&target, device, queue, size, width, height);

                    RenderResponse { data }
                }
            };
            match renderer_response_tx.send(response) {
                Ok(result) => (),
                Err(e) => panic!("Decoder connection lost. {}", e),
            };
        }
    });

    {
        let mut map = RENDERERS.write().unwrap();
        map.insert(
            key,
            SimpleContext {
                renderer_request_tx: renderer_request_tx,
                renderer_response_rx: renderer_response_rx,
            },
        );
    }
}

pub async fn test_render(x: f32, y: f32) -> Vec<u8> {
    let key: String = "key".to_string();

    let map = RENDERERS.read().unwrap();
    if !map.contains_key(&key) {
        panic!("Renderer not found. {}", key);
    }

    let context = &map[&key];

    match context.renderer_request_tx.send(RenderRequest {
        command: RenderCommand::Render(Pos {
            x: x.into(),
            y: y.into(),
        }),
    }) {
        Ok(result) => result,
        Err(e) => panic!("Renderer connection lost. {}", e),
    };
    let result = context
        .renderer_response_rx
        .recv()
        .expect("Failed to receive data");
    result.data
}

fn add_shapes_to_scene(scene: &mut Scene) {
    // Draw an outlined rectangle
    let stroke = Stroke::new(6.0);
    let rect = RoundedRect::new(10.0, 10.0, 240.0, 240.0, 20.0);
    let rect_stroke_color = Color::rgb(0.9804, 0.702, 0.5294);
    scene.stroke(&stroke, Affine::IDENTITY, rect_stroke_color, None, &rect);

    // Draw a filled circle
    let circle = Circle::new((420.0, 200.0), 120.0);
    let circle_fill_color = Color::rgb(0.9529, 0.5451, 0.6588);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        circle_fill_color,
        None,
        &circle,
    );

    // Draw a filled ellipse
    let ellipse = Ellipse::new((250.0, 420.0), (100.0, 160.0), -90.0);
    let ellipse_fill_color = Color::rgb(0.7961, 0.651, 0.9686);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        ellipse_fill_color,
        None,
        &ellipse,
    );

    // Draw a straight line
    let line = Line::new((260.0, 20.0), (620.0, 100.0));
    let line_stroke_color = Color::rgb(0.5373, 0.7059, 0.9804);
    scene.stroke(&stroke, Affine::IDENTITY, line_stroke_color, None, &line);

    // Draw a Bezier curve
    let bezier = vello::kurbo::BezPath::from_vec(vec![
        vello::kurbo::PathEl::MoveTo((200.0, 300.0).into()),
        vello::kurbo::PathEl::CurveTo(
            (250.0, 350.0).into(),
            (350.0, 250.0).into(),
            (400.0, 300.0).into(),
        ),
        vello::kurbo::PathEl::ClosePath,
    ]);
    let bezier_stroke_color = Color::rgb(0.4824, 0.7882, 0.4353);
    scene.stroke(
        &stroke,
        Affine::IDENTITY,
        bezier_stroke_color,
        None,
        &bezier,
    );

    // Draw a path
    let mut path = vello::kurbo::BezPath::new();
    path.move_to((100.0, 100.0));
    path.line_to((150.0, 150.0));
    path.quad_to((200.0, 100.0), (250.0, 150.0));
    path.curve_to((300.0, 200.0), (350.0, 150.0), (400.0, 200.0));
    path.close_path();
    let path_stroke_color = Color::rgb(0.8353, 0.2392, 0.3098);
    scene.stroke(&stroke, Affine::IDENTITY, path_stroke_color, None, &path);
}

enum RenderCommand {
    Render(Pos),
}

struct Pos {
    pub x: f64,
    pub y: f64,
}

struct RenderRequest {
    command: RenderCommand,
}

struct RenderResponse {
    data: Vec<u8>,
}

struct SimpleContext {
    renderer_request_tx: Sender<RenderRequest>,
    renderer_response_rx: Receiver<RenderResponse>,
}

unsafe impl Send for SimpleContext {}
unsafe impl Sync for SimpleContext {}

struct RenderInfo {
    width: u32,
    height: u32,
    data: Vec<u8>,
}
