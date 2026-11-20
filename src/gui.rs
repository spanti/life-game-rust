use crate::universe::{Coord, Snapshot};
use crate::sim::SimCommand;
use crossbeam::channel::{Receiver, Sender};
use piston_window::*;
use std::{collections::HashSet, sync::Arc};

const CELL_SIZE: f64 = 16.0;
const GRID_W: isize = 50;
const GRID_H: isize = 38;

pub fn run(state_rx: Receiver<Snapshot>, cmd_tx: Sender<SimCommand>) {
    let width_px = (GRID_W as f64 * CELL_SIZE) as u32;
    let height_px = (GRID_W as f64 * CELL_SIZE) as u32;

    let mut window: PistonWindow = WindowSettings::new(
        "Game of Life [Space: 暂停/继续 | Click: 编辑 | C: 清空]",
        [width_px, height_px],
    )
    .exit_on_esc(true)
    .build()
    .expect("无法创建 Piston 窗口");

    let mut current_cells: Snapshot = Arc::new(HashSet::new());
    let mut cursor_pos = [0.0, 0.0];

    // event loop
    while let Some(event) = window.next() {
        if let Ok(latest_cells) = state_rx.try_recv() {
            current_cells = latest_cells;
        }

        if let Some(pos) = event.mouse_cursor_args() {
            cursor_pos = pos;
        }

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

        if let Some(Button::Mouse(MouseButton::Left)) = event.press_args() {
            let gx = (cursor_pos[0] / CELL_SIZE).floor() as isize;
            let gy = (cursor_pos[1] / CELL_SIZE).floor() as isize;
            let _ = cmd_tx.send(SimCommand::ToggleCell((gx, gy)));
        }

        window.draw_2d(&event, | context, graphics, _device| {
            clear([0.11, 0.12, 0.14, 1.0], graphics);

            draw_grid(width_px as f64, height_px as f64, &context, graphics);
            draw_cells(&current_cells, &context, graphics);
        });
    }

    fn draw_grid<G: Graphics>(width: f64, height: f64, context: &Context, graphics: &mut G){
        let grid_color = [0.16, 0.17, 0.20, 1.0];
        for x in 0..=GRID_W {
            let px = x as f64 * CELL_SIZE;
            line(grid_color, 0.5, [px, 0.0, px, height], context.transform, graphics);
        }
        for y in 0..=GRID_H {
            let py = y as f64 * CELL_SIZE;
            line(grid_color, 0.5, [0.0, py, width, py], context.transform, graphics);
        }
    }

    fn draw_cells<G: Graphics>(cells: &HashSet<Coord>, context: &Context, graphics: &mut G) {
        let cell_color = [0.22, 0.85, 0.55, 1.0];
        for &(x, y) in cells {
            if (0..GRID_W).contains(&x) && (0..GRID_H).contains(&y) {
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
    }
}