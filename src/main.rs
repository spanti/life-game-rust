/*
Conway's Game of Life is a cellular automaton that is played on a 2D square grid. Each square (or "cell") on the grid can be either alive or dead, and they evolve according to the following rules:

Any live cell with fewer than two live neighbours dies (referred to as underpopulation).
Any live cell with more than three live neighbours dies (referred to as overpopulation).
Any live cell with two or three live neighbours lives, unchanged, to the next generation.
Any dead cell with exactly three live neighbours comes to life.
a method get live neighbours
live -> 1 dead -> 0
 */
use crossbeam::channel::{bounded, select, tick, unbounded};
use piston_window::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    thread,
    time::Duration,
};

mod universe; // 告诉编译器去加载同级目录下的 universe.rs

use universe::{Coord, Universe};

const CELL_SIZE: f64 = 16.0;
const GRID_W: isize = 50;
const GRID_H: isize = 38;

/// 发送给后台演化线程的控制指令
enum SimCommand {
    ToggleCell(Coord),
    TogglePause,
    Clear,
}

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
        let _ = state_tx.send(Arc::new(universe.alive.clone()));

        // 每 60ms 触发一次演化节拍
        let ticker = tick(Duration::from_millis(60));

        loop {
            select! {
                // 处理来自窗口的交互指令
                recv(cmd_rx) -> msg => {
                    match msg {
                        Ok(SimCommand::ToggleCell(coord)) => {
                            if !universe.alive.remove(&coord) {
                                universe.alive.insert(coord);
                            }
                            let _ = state_tx.try_send(Arc::new(universe.alive.clone()));
                        }
                        Ok(SimCommand::TogglePause) => {
                            paused = !paused;
                        }
                        Ok(SimCommand::Clear) => {
                            universe.alive.clear();
                            let _ = state_tx.try_send(Arc::new(universe.alive.clone()));
                        }
                        Err(_) => break, // 主窗口已关闭，退出后台线程
                    }
                }
                // 定时推进下一代
                recv(ticker) -> _ => {
                    if !paused {
                        universe.tick();
                        // 若渲染线程还没消费上一帧，try_send 不会死锁；若通道空闲则送入最新状态
                        let _ = state_tx.try_send(Arc::new(universe.alive.clone()));
                    }
                }
            }
        }
    });

    // 3. 初始化 Piston 窗口（主线程）
    let width_px = (GRID_W as f64 * CELL_SIZE) as u32;
    let height_px = (GRID_H as f64 * CELL_SIZE) as u32;

    let mut window: PistonWindow = WindowSettings::new(
        "Game of Life [Space: 暂停/继续 | Click: 编辑 | C: 清空]",
        [width_px, height_px],
    )
    .exit_on_esc(true)
    .build()
    .expect("无法创建 Piston 窗口");

    let mut current_cells = Arc::new(HashSet::new());
    let mut cursor_pos = [0.0, 0.0];

    // 4. 主渲染与事件循环
    while let Some(event) = window.next() {
        // 非阻塞拉取后台最新算好的一代
        if let Ok(latest_cells) = state_rx.try_recv() {
            current_cells = latest_cells;
        }

        // 记录鼠标位置
        if let Some(pos) = event.mouse_cursor_args() {
            cursor_pos = pos;
        }

        // 键盘输入处理
        if let Some(Button::Keyboard(key)) = event.press_args() {
            match key {
                Key::Space => {
                    let _ = cmd_tx.send(SimCommand::TogglePause);
                }
                Key::C => {
                    let _ = cmd_tx.send(SimCommand::Clear);
                }
                _ => {}
            }
        }

        // 鼠标左键点击切换细胞生死
        if let Some(Button::Mouse(MouseButton::Left)) = event.press_args() {
            let gx = (cursor_pos[0] / CELL_SIZE).floor() as isize;
            let gy = (cursor_pos[1] / CELL_SIZE).floor() as isize;
            let _ = cmd_tx.send(SimCommand::ToggleCell((gx, gy)));
        }

        // 2D 画面绘制
        window.draw_2d(&event, |context, graphics, _device| {
            // 深灰背景
            clear([0.11, 0.12, 0.14, 1.0], graphics);

            // 绘制辅助网格线
            let grid_color = [0.16, 0.17, 0.20, 1.0];
            for x in 0..=GRID_W {
                let px = x as f64 * CELL_SIZE;
                line(
                    grid_color,
                    0.5,
                    [px, 0.0, px, height_px as f64],
                    context.transform,
                    graphics,
                );
            }
            for y in 0..=GRID_H {
                let py = y as f64 * CELL_SIZE;
                line(
                    grid_color,
                    0.5,
                    [0.0, py, width_px as f64, py],
                    context.transform,
                    graphics,
                );
            }

            // 绘制视野内的活细胞（留出 1px 边距形成格子间隙）
            let cell_color = [0.22, 0.85, 0.55, 1.0];
            for &(x, y) in current_cells.iter() {
                if x >= 0 && x < GRID_W && y >= 0 && y < GRID_H {
                    rectangle(
                        cell_color,
                        [
                            x as f64 * CELL_SIZE + 1.0,
                            y as f64 * CELL_SIZE + 1.0,
                            CELL_SIZE - 2.0,
                            CELL_SIZE - 2.0,
                        ],
                        context.transform,
                        graphics,
                    );
                }
            }
        });
    }
}