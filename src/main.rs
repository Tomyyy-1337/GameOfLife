use image::ImageBuffer;
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use rayon::slice::{ParallelSlice, ParallelSliceMut};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use hashbrown::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, mpsc};
use std::thread;

const DIRS: [(i32,i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1,  0), (1,  0), (-1,  1), (0,  1), (1,  1)];

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Position {
    x: i32,
    y: i32,
}

impl Hash for Position {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let combined = ((self.x as i64) << 32) | (self.y as i64);
        combined.hash(state);
    }
}

#[derive(Debug)]
struct Grid {
    cells: HashMap<Position, u32>
}

impl Grid {
    fn from_str(str: &str) -> Grid {
        let mut cells = HashMap::new();
        str.split("#P").skip(1).for_each(|s| {
            let mut lines = s.lines();
            let mut pos = lines.next().unwrap().split_ascii_whitespace();
            let x = pos.next().unwrap().parse::<i32>().unwrap();
            let y = pos.next().unwrap().parse::<i32>().unwrap();
            for (i,line) in lines.enumerate() {
                for (j,_) in line.chars().enumerate().filter(|(_,c)| *c == '*') {
                    cells.insert(Position { x: x + j as i32, y: y + i as i32}, 1);
                }
            }
        });
        Grid {
            cells
        }
    }

    fn next(&self) -> Grid {
        let cells = self.cells.clone();
        let (tx, rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            let cells: HashMap<Position, u32> = cells.par_iter().filter(|(_, &v)| v < 100).map(|(pos, v)| (*pos, v + 1)).collect();
            tx.send(cells).unwrap();
        });

        let neighbors: Vec<_> = self.cells.par_iter().filter(|(_, v)| **v == 1)
            .map(|(pos, _)| {
                DIRS.iter().map(|(dx, dy)| {
                    Position { x: pos.x + dx, y: pos.y + dy} 
                }).collect::<Vec<Position>>()
            })
            .flatten()
            .fold(HashMap::new, |mut map, pos| {
                *map.entry(pos).or_insert(0) += 1;
                map
            })
            .reduce(HashMap::new, |mut map1, map2| {
                for (pos, count) in map2 {
                    *map1.entry(pos).or_insert(0) += count;
                }
                map1
            }).into_par_iter()
            .filter(|&(_, count)| count > 1 && count < 4)
            .collect();

        let mut cells = rx.recv().unwrap();
        neighbors.iter().for_each(|(pos, count)| {
            if *count == 3 || (*count == 2 && self.cells.get(pos) == Some(&1)) {
                cells.insert(*pos, 1);
            }
        });
        Grid { cells }
    }

    fn to_image(&self, width: i32, height: i32, center_x: i32, center_y: i32, pixel_per_cell: f64) -> ImageBuffer<image::Rgb<u8>, Vec<u8>> {
        let mut img: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::new(width as u32, height as u32);
        for (cell, &v) in &self.cells {
            let x_raw = (cell.x - center_x) as f64 * pixel_per_cell + width as f64 / 2.0;
            let y_raw = (cell.y - center_y) as f64 * pixel_per_cell + height as f64 / 2.0;
            let x = x_raw.round() as i32;
            let y = y_raw.round() as i32;
            if pixel_per_cell < 2.0 {
                if x >= 0 && x < width && y >= 0 && y < height {
                    img.put_pixel(x as u32, y as u32, Grid::color(v));
                }
                continue;
            }
            for i in 1..pixel_per_cell as i32 {
                for j in 1..pixel_per_cell as i32 {
                    if x + i >= 0 && x + i < width && y + j >= 0 && y + j < height {
                        img.put_pixel((x + i) as u32, (y + j) as u32, Grid::color(v));
                    }
                }
            }
        }

        img
    }

    fn color(val: u32) -> image::Rgb<u8> {
        match val {
            1 => image::Rgb([255, 255, 255]),
            2 => image::Rgb([0, 255, 255]),
            3 => image::Rgb([0, 100, 255]),
            4 => image::Rgb([0, 0, 255]),
            5 => image::Rgb([0, 0, 230]),
            6 => image::Rgb([0, 0, 200]),
            7 => image::Rgb([0, 0, 150]),
            8 => image::Rgb([0, 0, 100]),
            _ => image::Rgb([0, 0, 100 - val as u8]),
        }
    } 


    
}
fn main() -> Result<(), String> {
    let mut window_witdh = 800;
    let mut window_height = 600;

    let path = "input/test4.txt";
    let mut grid = Grid::from_str(&std::fs::read_to_string(path).unwrap());
    
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem.window("Game of Life", window_witdh, window_height)
        .position_centered()
        .resizable()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();
   
    let texture_creator = canvas.texture_creator();
    let mut texture: sdl2::render::Texture<'_> = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, window_witdh, window_height).unwrap();
    canvas.set_draw_color(Color::RGB(0, 255, 255));
    canvas.clear();
    canvas.present();

    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut zoom = 5.0;
    let mut x_pos: i32 = 0;
    let mut y_pos: i32 = 0;
    let mut prev_mouse_pos = None;

    'running: loop { 
        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event: sdl2::event::WindowEvent::SizeChanged(w, h), .. } => {
                    window_witdh = w as u32;
                    window_height = h as u32;
                    texture = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, window_witdh, window_height).unwrap();
                },
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                },
                Event::MouseWheel { y, .. } => {
                    zoom = zoom * 1.5f64.powi(y);
                },
                Event::MouseMotion { x, y, .. } => {
                    if let Some((prev_x, prev_y)) = prev_mouse_pos {
                        let dx = ((x - prev_x) as f64 / zoom * 1.3) as i32;
                        let dy = ((y - prev_y) as f64 / zoom * 1.3) as i32;
                        x_pos -= dx;
                        y_pos -= dy;
                        if dx != 0 && dy != 0 {
                            prev_mouse_pos = Some((x, y));
                        } else if dx != 0{
                            prev_mouse_pos = Some((x, prev_y));
                        } else if dy != 0{
                            prev_mouse_pos = Some((prev_x, y));
                        }
                        
                    }
                },
                Event::MouseButtonDown { x, y, .. } => {
                    prev_mouse_pos = Some((x, y));
                },
                Event::MouseButtonUp { .. } => {
                    prev_mouse_pos = None;
                },
                _ => {},
            }
        }
        canvas.clear();
        let img = grid.to_image(window_witdh as i32, window_height as i32, x_pos, y_pos, zoom);
        grid = grid.next();

        let img_data = img.into_raw();
        texture.update(None, &img_data, window_witdh as usize * 3).unwrap();
        canvas.copy(&texture, None, None).unwrap();
        canvas.present();
    }

    Ok(())
}