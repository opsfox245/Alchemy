use crate::camera::GPUObject;
use crate::texture;
use std::iter;
use winit::window::Window;
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

const RENDERFORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const SCUSAGE: wgpu::TextureUsage = wgpu::TextureUsage::RENDER_ATTACHMENT;
const SCPRESENT: wgpu::PresentMode = wgpu::PresentMode::Fifo;

pub struct State {
    surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    pub size: winit::dpi::PhysicalSize<u32>,
    depth_texture: texture::Texture,
    effect: Option<BasicEffect>, //This is initialized later.
}

impl State {
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::BackendBit::VULKAN);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Named Device"),
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
            },
            None, // Trace path
        ).await.unwrap();

        let sc_desc = wgpu::SwapChainDescriptor { usage: SCUSAGE, format: RENDERFORMAT, width: size.width, height: size.height, present_mode: SCPRESENT};

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);
        let depth_texture = texture::Texture::create_depth_texture(&device, &sc_desc, "depth_texture");

        Self {
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
            size,
            depth_texture,
            effect: None,
        }
    }

    pub fn add_effect(&mut self, effect: BasicEffect){ 
        self.effect = Some(effect);
    }

    pub fn get_effect(&self) -> &BasicEffect{
        return self.effect.as_ref().unwrap();
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.sc_desc.width = new_size.width;
        self.sc_desc.height = new_size.height;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
        self.depth_texture = texture::Texture::create_depth_texture(&self.device, &self.sc_desc, "depth_texture");
    }

    pub fn write_buffer(&mut self, buffer: &wgpu::Buffer, bytes: impl bytemuck::Pod ){
        self.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&[bytes]));
    }

    pub fn render(&mut self) -> Result<(), wgpu::SwapChainError> {
        let frame = self.swap_chain.get_current_frame()?.output;

        let mut encoder = self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder"),});

        {
            let mut render_pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });
            
            match &self.effect{
                Some(effect) => {
                    effect.render(&mut render_pass);
                },
                None => panic!("The Render pipeline was not initialized, please include init_pipleine somehwere in the code"),
            }
        }

        self.queue.submit(iter::once(encoder.finish()));

        Ok(())
    }
}

pub struct BasicEffect {
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_obj: GPUObject<crate::camera::Uniforms>,
}

impl BasicEffect {
    pub fn new(gpu: &State, camera_obj: GPUObject<crate::camera::Uniforms>) -> Self{
        let vs_module = gpu.device.create_shader_module(&wgpu::include_spirv!("shader.vert.spv"));
        let fs_module = gpu.device.create_shader_module(&wgpu::include_spirv!("shader.frag.spv"));

        let render_pipeline_layout =
        gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&camera_obj.layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main", // 1.
                buffers: &[], // 2.
            },
            fragment: Some(wgpu::FragmentState { // 3.
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState { // 4.
                    format: gpu.sc_desc.format,
                    alpha_blend: wgpu::BlendState::REPLACE,
                    color_blend: wgpu::BlendState::REPLACE,
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: wgpu::CullMode::Back,
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less, // 1.
                stencil: wgpu::StencilState::default(), // 2.
                bias: wgpu::DepthBiasState::default(),
                // Setting this to true requires Features::DEPTH_CLAMPING
                clamp_depth: false,
            }),
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
        });
        
        return Self{
            render_pipeline,
            camera_obj
        }
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>){
        render_pass.set_pipeline(&self.render_pipeline); // 2.
        render_pass.set_bind_group(self.camera_obj.binding, &self.camera_obj.bind_group, &[]); //TODO, the gpu object should know what its bind group is.
        render_pass.draw(0..3, 0..1); // 3.
    }

    pub fn write_buffer(&self, gpu: &State, buffer: &wgpu::Buffer, bytes: impl bytemuck::Pod ){
        gpu.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&[bytes]));
    }

    pub fn write_camera_buffer(&self, gpu: &State, bytes: impl bytemuck::Pod ){
        gpu.queue.write_buffer(&self.camera_obj.buffer, 0, bytemuck::cast_slice(&[bytes]));
    }

}