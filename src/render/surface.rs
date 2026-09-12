use super::context::GpuContext;
use super::quad::{QuadInstance, QuadPipeline};
use crate::widgets::window::VelvetWindow;
use wgpu::util::DeviceExt;

pub struct GpuSurface {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuSurface {
    pub fn new(ctx: &GpuContext, window: &VelvetWindow, width: u32, height: u32) -> Self {
        // SAFETY: VelvetWindow is kept alive at least as long as GpuSurface (owned together in the same struct).
        let surface = unsafe {
            std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(
                ctx.instance
                    .create_surface(window)
                    .expect("failed to create wgpu surface"),
            )
        };

        let caps = surface.get_capabilities(&ctx.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            color_space: wgpu::SurfaceColorSpace::Auto,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&ctx.device, &config);

        Self { surface, config }
    }

    pub fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&ctx.device, &self.config);
    }

    pub fn render(
        &self,
        ctx: &GpuContext,
        pipeline: &QuadPipeline,
        instances: &[QuadInstance],
        clear: wgpu::Color,
    ) {
        let surface_output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout => return,
            wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&ctx.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&ctx.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = surface_output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // update projection matrix uniform
        let ortho = create_ortho_matrix(self.config.width as f32, self.config.height as f32);
        ctx.queue
            .write_buffer(&pipeline.camera_buffer, 0, bytemuck::cast_slice(&[ortho]));

        // upload instance data
        let instance_buffer = if !instances.is_empty() {
            Some(
                ctx.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Instance Buffer"),
                        contents: bytemuck::cast_slice(instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        } else {
            None
        };

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Quad Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // bind pipeline and issue draw call if there are instances to draw
            if let Some(ref buffer) = instance_buffer {
                pass.set_pipeline(&pipeline.pipeline);
                pass.set_bind_group(0, &pipeline.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));

                // 6 vertices per quad (2 triangles), drawn once per instance
                pass.draw(0..6, 0..instances.len() as u32);
            }
        }

        ctx.queue.submit(Some(encoder.finish()));
        ctx.queue.present(surface_output);
    }
}

fn create_ortho_matrix(width: f32, height: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}
