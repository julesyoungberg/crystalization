use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc::channel;
use std::thread;

use nannou::prelude::*;
use rand::Rng;

const WIDTH: u32 = 889;
const HEIGHT: u32 = 500;

fn main() {
    nannou::app(model)
        .update(update)
        .simple_window(view)
        .size(WIDTH, HEIGHT)
        .run();
}

struct Model {
    walkers: Walkers,
    first_run: bool,
    main_window_id: WindowId,
    texture: wgpu::Texture,
    texture_capturer: wgpu::TextureCapturer,
    texture_reshaper: wgpu::TextureReshaper,
    renderer: nannou::draw::Renderer,
    image_sender: Sender<nannou::image::RgbaImage>,
    image_receiver: Receiver<nannou::image::RgbaImage>,
    draw: nannou::Draw,
}

fn model(app: &App) -> Model {
    let main_window_id = app.new_window().size(WIDTH, HEIGHT).view(view).build().unwrap();
    let window = app.window(main_window_id).unwrap();
    let device = window.device();
    let msaa_samples = window.msaa_samples();

    let size = pt2((WIDTH) as f32, (HEIGHT) as f32);
    let texture = create_app_texture(device, size, msaa_samples);
    let texture_reshaper = create_texture_reshaper(device, &texture, msaa_samples);

    // Create our `Draw` instance and a renderer for it.
    let draw = nannou::Draw::new();
    let descriptor = texture.descriptor();
    let renderer =
        nannou::draw::RendererBuilder::new().build_from_texture_descriptor(device, descriptor);

    // Create the texture capturer.
    let texture_capturer = wgpu::TextureCapturer::default();

    let desc = wgpu::CommandEncoderDescriptor {
        label: Some("nannou_isf_pipeline_new"),
    };
    let encoder = device.create_command_encoder(&desc);

    window.queue().submit([encoder.finish()]);

    let (image_sender, image_receiver) = channel();

    Model {
        walkers: Walkers::new(0.5, size[0], size[1]),
        first_run: true,
        main_window_id,
        texture,
        texture_capturer,
        texture_reshaper,
        renderer,
        image_sender,
        image_receiver,
        draw,
    }
}

fn update(app: &App, model: &mut Model, _update: Update) {
    if let Ok(image) = model.image_receiver.try_recv() {
        // let path = app.project_path().unwrap().join("frame").with_extension("png");
        // image.save(path).ok();
        model.walkers.update(&image);
    }
    
    // prepare to draw.
    let draw = &model.draw;
    draw.reset();

    // clear the background on the first run
    if model.first_run {
        draw.background().color(BLACK);
        model.first_run = false;
    }

    model.walkers.draw(&draw);

    let window = app.window(model.main_window_id).unwrap();
    let device = window.device();

    // setup encoder
    let desc = wgpu::CommandEncoderDescriptor {
        label: Some("render_pass"),
    };
    let mut encoder = device.create_command_encoder(&desc);

    model
        .renderer
        .render_to_texture(device, &mut encoder, &draw, &model.texture);

    // Take a snapshot of the texture. The capturer will do the following:
    //
    // 1. Resolve the texture to a non-multisampled texture if necessary.
    // 2. Convert the format to non-linear 8-bit sRGBA ready for image storage.
    // 3. Copy the result to a buffer ready to be mapped for reading.
    let snapshot = model
        .texture_capturer
        .capture(device, &mut encoder, &model.texture);

    // Submit the commands for our drawing and texture capture to the GPU.
    window.queue().submit(Some(encoder.finish()));

    let sender = model.image_sender.clone();

    snapshot
        .read(move |result| {
            let image = result.expect("failed to map texture memory").to_owned();
            sender.send(image).ok();
        })
        .unwrap();
}

fn view(app: &App, model: &Model, frame: Frame) {
    // Sample the texture and write it to the frame.
    let mut encoder = frame.command_encoder();
    model
        .texture_reshaper
        .encode_render_pass(frame.texture_view(), &mut *encoder);
}

struct Walkers {
    walkers: Vec<Walker>,
    turn_chance: f32,
    turn_angle: f32,
    division_chance: f32,
    division_angle: f32,
    speed: f32,
    width: f32,
    height: f32,
    kill_threshold: u8,
    line_weight: f32,
}

impl Walkers {
    pub fn new(speed: f32, width: f32, height: f32) -> Self {
        Self {
            walkers: vec![Walker::new(pt2(0.0, height * -0.5), pt2(0.0, 1.0)), Walker::new(pt2(width * -0.5, 0.0), pt2(1.0, 0.0))],
            turn_chance: 0.01,
            turn_angle: 1.0471975512, // pi / 3
            division_chance: 0.000000,
            division_angle: 0.7853981634, // pi / 4
            speed,
            width,
            height,
            kill_threshold: 150,
            line_weight: 1.0,
        }
    }

    pub fn update(&mut self, prev_frame: &nannou::image::RgbaImage) {
        let (tx, rx) = channel();
        let mut children = vec![];

        for w in self.walkers.iter() {
            let mut walker = w.clone();
            // turn walkers
            let thread_tx = tx.clone();
            let img = prev_frame.clone();
            let width = self.width;
            let height = self.height;
            let turn_chance = self.turn_chance;
            let turn_angle = self.turn_angle;
            let division_chance = self.division_chance;
            let division_angle = self.division_angle;
            let speed = self.speed;
            let kill_threshold = self.kill_threshold;

            let child = thread::spawn(move || {
                let mut new_walkers = vec![];
                let img_width = img.width();
                let img_height = img.height();

                let turn_value = rand::thread_rng().gen_range(0..100) as f32 / 100.0;
                if turn_value < turn_chance {
                    walker.turn(turn_angle);
                }

                // divide walkers
                let div_value = rand::thread_rng().gen_range(0..100) as f32 / 100.0;
                if div_value < division_chance {
                    let mut child = walker.clone();
                    child.turn(division_angle);
                    new_walkers.push(child);
                }

                // update walker position
                walker.update(speed);

                // wrap around canvas
                let hwidth = width / 2.0;
                if walker.position.x >= hwidth {
                    walker.position.x -= width;
                    walker.prev_position = walker.position;
                } else if walker.position.x <= -hwidth {
                    walker.position.x += width;
                    walker.prev_position = walker.position;
                }

                let hheight = height / 2.0;
                if walker.position.y >= hheight {
                    walker.position.y -= height;
                    walker.prev_position = walker.position;
                } else if walker.position.y <= -hheight {
                    walker.position.y += height;
                    walker.prev_position = walker.position;
                }

                let pixel_x = map(walker.position.x, -hwidth, hwidth, 0.0, img_width as f32) as u32;
                let pixel_y =
                    map(walker.position.y, -hheight, hheight, 0.0, img_height as f32) as u32;
                let pixel = img.get_pixel(
                    pixel_x.min(img_width - 1),
                    img_height - 1 - pixel_y.min(img_height - 1),
                );
                
                let avg = (pixel[0] + pixel[1] + pixel[3]) / 3;
                println!("{:?}", avg);
                if avg >= kill_threshold {
                    walker.dead = true;
                }

                new_walkers.push(walker);

                thread_tx.send(new_walkers).unwrap();
            });

            children.push(child);
        }

        self.walkers = vec![];
        for _ in 0..children.len() {
            let mut new_walkers: Vec<Walker> = rx
                .recv()
                .unwrap()
                .iter()
                .filter(|w| !w.dead)
                .map(|w| w.clone())
                .collect();
            self.walkers.append(&mut new_walkers);
        }

        for child in children {
            child.join().expect("oops! the child thread panicked");
        }
    }

    pub fn draw(&self, draw: &Draw) {
        for walker in self.walkers.iter() {
            walker.draw(draw, self.line_weight);
        }
    }
}

fn map(i: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    return (i - in_min) / (in_max - in_min) * (out_max - out_min) + out_min;
}

#[derive(Debug, Clone)]
struct Walker {
    pub position: Vec2,
    pub prev_position: Vec2,
    pub velocity: Vec2,
    pub dead: bool,
}

impl Walker {
    pub fn new(position: Vec2, velocity: Vec2) -> Self {
        Self {
            position,
            prev_position: position,
            velocity,
            dead: false,
        }
    }

    pub fn turn(&mut self, angle: f32) {
        let factor = rand::thread_rng().gen_range(0..100) as f32 / 100.0 * 2.0 - 1.0;
        self.velocity = self.velocity.rotate(angle * factor);
    }

    pub fn next_position(&mut self, speed: f32) -> Vec2 {
        pt2(
            self.position.x + self.velocity.x * speed,
            self.position.y + self.velocity.y * speed,
        )
    }

    pub fn update(&mut self, speed: f32) {
        self.prev_position = self.position.clone();
        self.position = self.next_position(speed);
    }

    pub fn draw(&self, draw: &Draw, weight: f32) {
        draw.line()
            .start(self.prev_position)
            .end(self.position)
            .weight(weight)
            .color(WHITE);
    }
}

fn create_app_texture(device: &wgpu::Device, size: Point2, msaa_samples: u32) -> wgpu::Texture {
    wgpu::TextureBuilder::new()
        .size([size[0] as u32, size[1] as u32])
        .usage(
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        )
        .sample_count(msaa_samples)
        .format(Frame::TEXTURE_FORMAT)
        .build(device)
}

fn create_texture_reshaper(
    device: &wgpu::Device,
    texture: &wgpu::Texture,
    msaa_samples: u32,
) -> wgpu::TextureReshaper {
    let texture_view = texture.view().build();
    let texture_component_type = texture.sample_type();
    let dst_format = Frame::TEXTURE_FORMAT;
    wgpu::TextureReshaper::new(
        device,
        &texture_view,
        msaa_samples,
        texture_component_type,
        msaa_samples,
        dst_format,
    )
}
