/*
Conway's Game of Life is a cellular automaton that is played on a 2D square grid. Each square (or "cell") on the grid can be either alive or dead, and they evolve according to the following rules:

Any live cell with fewer than two live neighbours dies (referred to as underpopulation).
Any live cell with more than three live neighbours dies (referred to as overpopulation).
Any live cell with two or three live neighbours lives, unchanged, to the next generation.
Any dead cell with exactly three live neighbours comes to life.
a method get live neighbours
live -> 1 dead -> 0
 */
use std::{thread, time::Duration};

struct Universe {
    width: usize,
    height: usize,
    current: Vec<u8>,
    next: Vec<u8>
}

impl Universe {
    fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            current: vec![0; size],
            next: vec![0; size]
        }
    }

    #[inline]
    fn get_index(&self, x: usize, y:usize) -> usize {
        y * self.width + x
    }

    fn set_alive(&mut self, x: usize, y: usize) {
        let idx = self.get_index(x % self.width, y % self.height);
        self.current[idx] = 1;
    }

    fn live_neighbor_count(&self, x: usize, y: usize) -> u8 {
        let mut count = 0;

        // 遍历周围 3x3 区域
        for dy in [self.height - 1, 0, 1] {
            for dx in [self.width - 1, 0, 1] {
                // 跳过自身
                if dx == 0 && dy == 0 {
                    continue;
                }

                let nx = (x + dx) % self.width;
                let ny = (y + dy) % self.height;
                let idx = self.get_index(nx, ny);
                count += self.current[idx];
            }
        }
        count
    }

    fn tick(&mut self){
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = self.get_index(x, y);
                let cell = self.current[idx];
                let neighbor = self.live_neighbor_count(x, y);

                let next_cell = match(cell, neighbor){
                    (1, x) if x < 2 => 0,
                    (1, 2) | (1, 3) => 1,
                    (1, x) if x > 3 => 0,
                    (0, 3) => 1,
                    (otherwise, _) => otherwise,
                };

                self.next[idx] = next_cell;
            }
        }

        std::mem::swap(&mut self.current, &mut self.next);
    }

    // TODO: rewrite it with some gui libraries 
    fn render(&self) {
        // 先拼接到字符串再一次性输出，避免逐格打印导致的屏幕闪烁
        let mut buffer = String::with_capacity((self.width + 1) * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let symbol = if self.current[self.get_index(x, y)] == 1 {
                    "◼ "
                } else {
                    "· "
                };
                buffer.push_str(symbol);
            }
            buffer.push('\n');
        }
        // ANSI 转义码：清空屏幕并将光标移回 (1, 1)
        print!("\x1B[2J\x1B[H{}", buffer);
    }

    // add a method to load a universe from file....
}

fn main(){
    let mut universe = Universe::new(30, 20);

    // 放置一个滑翔机 (Glider)
    // . O .
    // . . O
    // O O O
    universe.set_alive(2, 1);
    universe.set_alive(3, 2);
    universe.set_alive(1, 3);
    universe.set_alive(2, 3);
    universe.set_alive(3, 3);

    loop {
        universe.render();
        universe.tick();
        thread::sleep(Duration::from_millis(100));
    }
}