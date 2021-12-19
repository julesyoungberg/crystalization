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
}

fn model(_app: &App) -> Model {
    Model {
        walkers: Walkers::new(0.5),
        first_run: true,
    }
}

fn update(app: &App, model: &mut Model, _update: Update) {
    // read previous frame
    let image = match nannou::image::io::Reader::open(captured_frame_path(app)) {
        Ok(f) => match f.decode() {
            Ok(i) => i,
            Err(_) => return,
        },
        Err(_) => return,
    };

    model.walkers.update(&image);
    model.first_run = false;
}

fn view(app: &App, model: &Model, frame: Frame) {
    // Prepare to draw.
    let draw = app.draw();

    // Clear the background.
    if model.first_run {
        draw.background().color(BLACK);
    }

    // draw state
    model.walkers.draw(&draw);

    // Write to the window frame.
    draw.to_frame(app, &frame).unwrap();

    // Capture the frame!
    let file_path = captured_frame_path(app);
    let window = app.main_window();
    window.capture_frame(file_path);

    // let frame_capture = window
    //     .frame_data
    //     .as_ref()
    //     .expect("window capture requires that `view` draws to a `Frame` (not a `RawFrame`)")
    //     .capture;
    // println!("frame_capture: {:?}", frame_capture)
}

fn captured_frame_path(app: &App) -> std::path::PathBuf {
    // Create a path that we want to save this frame to.
    app.project_path()
        .expect("failed to locate `project_path`")
        // Name each file after the number of the frame.
        .join("frame")
        // The extension will be PNG. We also support tiff, bmp, gif, jpeg, webp and some others.
        .with_extension("png")
}

struct Walkers {
    walkers: Vec<Walker>,
    turn_chance: f32,
    turn_angle: f32,
    division_chance: f32,
    division_angle: f32,
    speed: f32,
}

impl Walkers {
    pub fn new(speed: f32) -> Self {
        Self {
            walkers: vec![Walker::new(pt2(0.0, HEIGHT as f32 * -0.5), pt2(0.0, 1.0))],
            turn_chance: 0.01,
            turn_angle: 1.0471975512, // pi / 3
            division_chance: 0.00001,
            division_angle: 0.7853981634, // pi / 4
            speed,
        }
    }

    pub fn update(&mut self, prev_frame: &nannou::image::DynamicImage) {
        let (tx, rx) = channel();
        let mut children = vec![];

        for w in self.walkers.iter() {
            let mut walker = w.clone();
            // turn walkers
            let thread_tx = tx.clone();
            let img = prev_frame.to_rgba8();
            let hwidth = WIDTH as f32 / 2.0;
            let hheight = HEIGHT as f32 / 2.0;
            let turn_chance = self.turn_chance;
            let turn_angle = self.turn_angle;
            let division_chance = self.division_chance;
            let division_angle = self.division_angle;
            let speed = self.speed;

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
                if walker.position.x >= hwidth {
                    walker.position.x -= WIDTH as f32;
                    walker.prev_position = walker.position;
                } else if walker.position.x <= -hwidth {
                    walker.position.y += WIDTH as f32;
                    walker.prev_position = walker.position;
                }

                if walker.position.y >= hheight {
                    walker.position.y -= HEIGHT as f32;
                    walker.prev_position = walker.position;
                } else if walker.position.y <= -hheight {
                    walker.position.y += HEIGHT as f32;
                    walker.prev_position = walker.position;
                }

                // println!("walker position: {:?}", walker.position);
                let pixel_x = map(walker.position.x, -hwidth, hwidth, 0.0, img_width as f32) as u32;
                let pixel_y =
                    map(walker.position.y, -hheight, hheight, 0.0, img_height as f32) as u32;
                // println!("pixel pos: {:?}, {:?}", pixel_x, pixel_y);
                let pixel = img.get_pixel(
                    pixel_x.min(img_width - 1),
                    img_height - 1 - pixel_y.min(img_height - 1),
                );
                // println!("pixel: {:?}", pixel);

                if pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255 {
                    walker.dead = true;
                }

                new_walkers.push(walker);

                thread_tx.send(new_walkers).unwrap();
            });

            children.push(child);
        }

        self.walkers = vec![];
        for _ in 0..self.walkers.len() {
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
            walker.draw(draw);
        }
    }
}

fn map(i: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    return (i - in_min) / (in_max - in_min) * (out_max - out_min) + out_min;
}

#[derive(Clone)]
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

    pub fn draw(&self, draw: &Draw) {
        draw.line()
            .start(self.prev_position)
            .end(self.position)
            .weight(1.0)
            .color(WHITE);
    }
}
