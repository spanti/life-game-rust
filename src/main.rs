/*
Conway's Game of Life is a cellular automaton that is played on a 2D square grid. Each square (or "cell") on the grid can be either alive or dead, and they evolve according to the following rules:

Any live cell with fewer than two live neighbours dies (referred to as underpopulation).
Any live cell with more than three live neighbours dies (referred to as overpopulation).
Any live cell with two or three live neighbours lives, unchanged, to the next generation.
Any dead cell with exactly three live neighbours comes to life.
a method get live neighbours
live -> 1 dead -> 0
 */
mod universe; // 告诉编译器去加载同级目录下的 universe.rs
mod gui;

use std::sync::Arc;
use std::collections::HashSet;
use crate::universe::Coord;

use crossbeam::channel::{bounded, select, tick, unbounded};
use gui::SimCommand;
use std::{thread, time::Duration};
use universe::{Snapshot, Universe};

fn main() {
    // 1. 建立跨线程通信通道
    // state_tx/rx: 容量为 1 的有界通道，充当线程安全的双缓冲，避免后台无限堆积帧
    let (state_tx, state_rx) = bounded::<Arc<HashSet<Coord>>>(1);
    // cmd_tx/rx: UI 线程向模拟线程发送交互指令
    let (cmd_tx, cmd_rx) = unbounded::<SimCommand>();

    // 2. 启动后台演化线程
    thread::spawn(move || {
        // 初始化一个经典的高斯帕滑翔机枪（Gosper Glider Gun）
        let glider_gun = [
            (1, 5), (1, 6), (2, 5), (2, 6),
            (11, 5), (11, 6), (11, 7), (12, 4), (12, 8), (13, 3), (13, 9),
            (14, 3), (14, 9), (15, 6), (16, 4), (16, 8), (17, 5), (17, 6),
            (17, 7), (18, 6),
            (21, 3), (21, 4), (21, 5), (22, 3), (22, 4), (22, 5), (23, 2),
            (23, 6), (25, 1), (25, 2), (25, 6), (25, 7),
            (35, 3), (35, 4), (36, 3), (36, 4),
        ];
        let mut universe = Universe::from_cells(glider_gun);
        let mut paused = false;

        // 推送初始帧
        let _ = state_tx.send(universe.snapshot());

        // 每 60ms 触发一次演化节拍
        let ticker = tick(Duration::from_millis(60));

        loop {
            let mut dirty = false;

            select! {
                recv(cmd_rx) -> msg => match msg {
                    Ok(SimCommand::ToggleCell(coord)) => {
                        universe.toggle_cell(coord);
                        dirty = true;
                    }
                    Ok(SimCommand::TogglePause) => paused = !paused,
                    Ok(SimCommand::Clear) => {
                        universe.clear();
                        dirty = true;
                    }
                    Err(_) => break,
                },
                recv(ticker) -> _  => {
                    if !paused {
                        universe.tick();
                        dirty = true;
                    }
                }
            }

            // 仅在状态实际发生改变时统一推送一次快照
            if dirty {
                let _ = state_tx.try_send(universe.snapshot());
            }
        }
    });

    // main thread start gui cycle
    gui::run(state_rx, cmd_tx);
}