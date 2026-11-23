/*
Conway's Game of Life is a cellular automaton that is played on a 2D square grid. Each square (or "cell") on the grid can be either alive or dead, and they evolve according to the following rules:

Any live cell with fewer than two live neighbours dies (referred to as underpopulation).
Any live cell with more than three live neighbours dies (referred to as overpopulation).
Any live cell with two or three live neighbours lives, unchanged, to the next generation.
Any dead cell with exactly three live neighbours comes to life.
a method get live neighbours
live -> 1 dead -> 0

download rle file from this wiki: https://conwaylife.com/wiki/
 */
mod universe; // 告诉编译器去加载同级目录下的 universe.rs
mod gui;
mod sim;
mod pattern;

use crossbeam::channel::{bounded, unbounded};
use sim::Simulation;
use std::{thread, time::Duration};
use universe::Universe;

fn main() {
    let (state_tx, state_rx) = bounded(1);
    let (cmd_tx, cmd_rx) = unbounded();

    let glider_gun = [
        (1, 5), (1, 6), (2, 5), (2, 6),
        (11, 5), (11, 6), (11, 7), (12, 4), (12, 8), (13, 3), (13, 9),
        (14, 3), (14, 9), (15, 6), (16, 4), (16, 8), (17, 5), (17, 6),
        (17, 7), (18, 6),
        (21, 3), (21, 4), (21, 5), (22, 3), (22, 4), (22, 5), (23, 2),
        (23, 6), (25, 1), (25, 2), (25, 6), (25, 7),
        (35, 3), (35, 4), (36, 3), (36, 4),
    ];
    let universe = Universe::from_cells(glider_gun);

    thread::spawn(move || {
        Simulation::new(universe, Duration::from_millis(60), cmd_rx, state_tx).run();
    });

    gui::App::new(state_rx, cmd_tx).run();
}