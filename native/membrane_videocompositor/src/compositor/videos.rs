use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use wgpu::util::DeviceExt;

use super::colour_converters::YUVToRGBAConverter;

use super::texture_transformations::{
    texture_transformers::TextureTransformer, TextureTransformationName,
    TextureTransformationUniform,
};
use super::textures::{RGBATexture, YUVTextures};
use super::{Vec2d, Vertex};

#[derive(Debug, Clone, Copy)]
// All of the fields are in pixels, except of the `z`, which should be from the <0, 1> range
pub struct VideoProperties {
    /// Position in pixels.
    /// Specifying a position outside of the `output_caps`
    /// of the scene this will be rendered onto will cause it to not be displayed.
    pub resolution: Vec2d<u32>,
    pub placement: VideoPlacement,
}

#[derive(Debug, Clone, Copy)]
pub struct VideoPlacement {
    pub position: Vec2d<u32>,
    pub size: Vec2d<u32>,
    pub z: f32,
}

#[derive(Debug)]
pub enum Message {
    Frame { pts: u64, frame: RGBATexture },
    EndOfStream,
}

pub enum DrawResult {
    /// Contains the pts of the rendered frame
    Rendered(u64),
    NotRendered,
    EndOfStream,
}

#[rustfmt::skip]
const INDICES: [u16; 6] = [
    0, 1, 3,
    1, 2, 3
];

#[derive(Debug)]
pub struct InputVideo {
    frames: VecDeque<Message>,
    yuv_textures: YUVTextures,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    pub properties: VideoProperties,
    pub texture_transformations: Vec<TextureTransformationUniform>,
    previous_frame: Option<Message>,
    single_texture_bind_group_layout: Arc<wgpu::BindGroupLayout>,
    /// When a video is created this is set to `true`. When `draw` is later called on it,
    /// until the first frame form this video's queue is composed, it won't block the compositor
    /// while it's frames are considered 'too new'. When the first frame from this video is composed,
    /// this gets set to `false` and the video operates normally.
    was_just_added: bool,
}

impl InputVideo {
    pub fn new(
        device: &wgpu::Device,
        single_texture_bind_group_layout: Arc<wgpu::BindGroupLayout>,
        all_textures_bind_group_layout: &wgpu::BindGroupLayout,
        properties: VideoProperties,
        texture_transformations: Vec<TextureTransformationUniform>,
    ) -> Self {
        let yuv_textures = YUVTextures::new(
            device,
            properties.resolution.x,
            properties.resolution.y,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            Some(&single_texture_bind_group_layout),
            Some(all_textures_bind_group_layout),
        );

        let frames = VecDeque::new();

        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("video vertex buffer"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            size: std::mem::size_of::<Vertex>() as u64 * 4,
            mapped_at_creation: false,
        });

        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("video index buffer"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self::update_texture_transformations(properties, &mut texture_transformations.to_vec());

        Self {
            yuv_textures,
            frames,
            vertices,
            indices,
            properties,
            texture_transformations,
            previous_frame: None,
            single_texture_bind_group_layout,
            was_just_added: true,
        }
    }

    pub fn update_properties(
        &mut self,
        device: &wgpu::Device,
        single_texture_bind_group_layout: Arc<wgpu::BindGroupLayout>,
        all_textures_bind_group_layout: &wgpu::BindGroupLayout,
        properties: VideoProperties,
        texture_transformations: Option<Vec<TextureTransformationUniform>>,
    ) {
        let yuv_textures = YUVTextures::new(
            device,
            properties.resolution.x,
            properties.resolution.y,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            Some(&single_texture_bind_group_layout),
            Some(all_textures_bind_group_layout),
        );
        self.yuv_textures = yuv_textures;
        self.properties = properties;
        match texture_transformations {
            Some(mut texture_transformations) => {
                self.texture_transformations =
                    Self::update_texture_transformations(properties, &mut texture_transformations)
                        .to_vec();
            }
            None => {
                self.texture_transformations = Self::update_texture_transformations(
                    properties,
                    &mut self.texture_transformations,
                )
                .to_vec();
            }
        }
    }

    pub fn update_texture_transformations(
        properties: VideoProperties,
        texture_transformations: &mut Vec<TextureTransformationUniform>,
    ) -> &Vec<TextureTransformationUniform> {
        let mut transformed_video_properties = properties.clone();

        for texture_transformation in texture_transformations.iter_mut() {
            transformed_video_properties =
                texture_transformation.update_video_properties(transformed_video_properties);
        }
        texture_transformations
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upload_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        converter: &YUVToRGBAConverter,
        data: &[u8],
        pts: u64,
        last_rendered_pts: Option<u64>,
        texture_transformers: &HashMap<TextureTransformationName, TextureTransformer>,
    ) {
        self.yuv_textures.upload_data(queue, data);
        let mut frame = RGBATexture::new(
            device,
            self.properties.resolution.x,
            self.properties.resolution.y,
            &self.single_texture_bind_group_layout,
        );
        converter.convert(device, queue, &self.yuv_textures, &frame);

        // Runs all texture transformations.
        for transformation_uniform in self.texture_transformations.iter() {
            let transformed_frame = RGBATexture::new(
                device,
                self.properties.resolution.x,
                self.properties.resolution.y,
                &self.single_texture_bind_group_layout,
            );

            let texture_transformer =
                transformation_uniform.get_texture_transformer(texture_transformers);

            texture_transformer.transform(
                device,
                queue,
                &frame,
                &transformed_frame,
                transformation_uniform,
            );

            frame = transformed_frame;
        }

        // if we haven't rendered a frame yet, or pts of our frame is ahead of last rendered frame
        if last_rendered_pts.is_none() || pts > last_rendered_pts.unwrap() {
            // then we can add the frame to the queue (we assume the frames come in order)
            self.frames.push_back(Message::Frame { frame, pts });
        }
        // otherwise, our frame is too old to be added to the queue, so we check if it is newer than the previously used frame,
        // which is our fallback in case we are forced to render before a new enough frame arrives.
        else if let Some(Message::Frame { pts: prev_pts, .. }) = self.previous_frame.as_ref() {
            if *prev_pts < pts {
                self.previous_frame = Some(Message::Frame { frame, pts });
            }
        }
    }

    pub fn vertex_data(&self, output_caps: &crate::RawVideo) -> [Vertex; 4] {
        let scene_width = output_caps.width;
        let scene_height = output_caps.height;

        let position = self.properties.placement.position;
        let width = self.properties.placement.size.x;
        let height = self.properties.placement.size.y;

        let left = lerp(
            self.properties.placement.position.x as f64,
            0.0,
            scene_width.get() as f64,
            -1.0,
            1.0,
        ) as f32;
        let right = lerp(
            position.x as f64 + width as f64,
            0.0,
            scene_width.get() as f64,
            -1.0,
            1.0,
        ) as f32;
        let top = lerp(position.y as f64, 0.0, scene_height.get() as f64, 1.0, -1.0) as f32;
        let bot = lerp(
            position.y as f64 + height as f64,
            0.0,
            scene_height.get() as f64,
            1.0,
            -1.0,
        ) as f32;

        [
            Vertex {
                position: [right, top, self.properties.placement.z],
                texture_coords: [1.0, 0.0],
            },
            Vertex {
                position: [left, top, self.properties.placement.z],
                texture_coords: [0.0, 0.0],
            },
            Vertex {
                position: [left, bot, self.properties.placement.z],
                texture_coords: [0.0, 1.0],
            },
            Vertex {
                position: [right, bot, self.properties.placement.z],
                texture_coords: [1.0, 1.0],
            },
        ]
    }

    pub fn front_pts(&self) -> Option<u64> {
        if let Some(Message::Frame { pts, .. }) = self.frames.front() {
            Some(*pts)
        } else {
            None
        }
    }

    pub fn properties(&self) -> &VideoProperties {
        &self.properties
    }

    /// This returns pts of the used frame
    pub fn draw<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass<'a>,
        output_caps: &crate::RawVideo,
        frame_interval: Option<(u64, u64)>,
    ) -> DrawResult {
        queue.write_buffer(
            &self.vertices,
            0,
            bytemuck::cast_slice(&self.vertex_data(output_caps)),
        );

        let (frame, pts) = match self.frames.front() {
            Some(Message::Frame { frame, pts }) => {
                // this is the case when the video was just added and its frames are 'too new'
                if let Some((_, end)) = frame_interval {
                    if *pts > end && self.was_just_added {
                        return DrawResult::NotRendered;
                    }
                }

                (frame, *pts)
            }

            Some(Message::EndOfStream) => return DrawResult::EndOfStream,

            None => match self.previous_frame.as_ref() {
                Some(Message::Frame { pts, frame }) => (frame, *pts),

                Some(Message::EndOfStream) => return DrawResult::EndOfStream,

                None => return DrawResult::NotRendered,
            },
        };

        render_pass.set_bind_group(0, frame.texture.bind_group.as_ref().unwrap(), &[]);

        render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.set_vertex_buffer(0, self.vertices.slice(..));

        let indices_len = (self.indices.size() / std::mem::size_of::<u16>() as u64) as u32;

        render_pass.draw_indexed(0..indices_len, 0, 0..1);

        self.was_just_added = false;

        DrawResult::Rendered(pts)
    }

    pub fn remove_stale_frames(&mut self, interval: Option<(u64, u64)>) {
        while self.is_front_frame_too_old(interval) {
            self.pop_frame();
        }
    }

    pub fn pop_frame(&mut self) {
        if let Some(Message::Frame { pts, frame }) = self.frames.pop_front() {
            self.previous_frame = Some(Message::Frame { pts, frame });
        }
    }

    pub fn send_end_of_stream(&mut self) {
        self.frames.push_back(Message::EndOfStream);
    }

    pub fn is_front_frame_too_old(&self, interval: Option<(u64, u64)>) -> bool {
        if let Some(Message::EndOfStream) = self.frames.front() {
            return false;
        }

        if interval.is_none() || self.front_pts().is_none() {
            return false;
        }

        self.front_pts().is_some() && interval.unwrap().0 > self.front_pts().unwrap()
    }

    pub fn is_frame_ready(&self, interval: Option<(u64, u64)>) -> bool {
        if let Some(Message::EndOfStream) = self.frames.front() {
            return true;
        }

        // if the stream hasn't ended then we have to have a frame in the queue, then either:
        if self.front_pts().is_some() {
            // this is the first frame, which means a frame with any pts is good
            if interval.is_none() {
                return true;
            }

            // or we have to fit between the start and end pts
            if interval.unwrap().0 <= self.front_pts().unwrap()
                && self.front_pts().unwrap() <= interval.unwrap().1
            {
                return true;
            }

            // or this video was just added, and frames in it's queue are 'too new'
            if self.was_just_added {
                return true;
            }
        }

        false
    }
}

/// Maps point `x` from the domain \[`x_min`, `x_max`\] to the point in the \[`y_min, y_max`\] line segment, using linear interpolation.
///
/// `x` outside the original domain will be extrapolated outside the target domain.
fn lerp(x: f64, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> f64 {
    (x - x_min) / (x_max - x_min) * (y_max - y_min) + y_min
}
