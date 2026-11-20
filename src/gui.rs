use crate::{
    sim::SimCommand, universe::{Coord, Rule, Snapshot},
};
use crossbeam::channel::{Receiver, Sender};
use piston_window::*;
use std::{collections::HashSet, sync::Arc};
use crate::pattern::Pattern;

const MIN_CELL_SIZE: f64 = 2.0;
const MAX_CELL_SIZE: f64 = 64.0;
const DEFAULT_CELL_SIZE: f64 = 16.0;

// 2d camera
struct Camera {
    offset: [f64; 2],
    cell_size: f64
}

impl Camera {
    fn new() -> Self {
        Self {
            offset: [100.0, 100.0],
            cell_size: DEFAULT_CELL_SIZE
        }
    }

    fn reset(&mut self) {
        self.offset = [100.0, 100.0];
        self.cell_size = DEFAULT_CELL_SIZE;
    }

    fn screen_to_world(&self, screen: [f64; 2]) -> Coord {
        let wx = ((screen[0] - self.offset[0]) / self.cell_size).floor() as isize;
        let wy = ((screen[1] - self.offset[1]) / self.cell_size).floor() as isize;
        (wx, wy)
    }

    fn world_to_screen(&self, (wx, wy): Coord) -> [f64; 2] {
        [
            wx as f64 * self.cell_size + self.offset[0],
            wy as f64 * self.cell_size + self.offset[1],
        ]
    }

    fn zoom_at(&mut self, cursor: [f64; 2], factor: f64){ 
        let old_size = self.cell_size;
        let new_size = (old_size * factor).clamp(MIN_CELL_SIZE, MAX_CELL_SIZE);
        let ratio = new_size / old_size;

        self.offset[0] = cursor[0] - (cursor[0] - self.offset[0]) * ratio;
        self.offset[1] = cursor[1] - (cursor[1] - self.offset[1]) * ratio;
        self.cell_size = new_size;
    }

    fn visible_bounds(&self, win_w: f64, win_h: f64) -> (isize, isize, isize, isize) {
        let min_x = ((-self.offset[0]) / self.cell_size).floor() as isize;
        let max_x = ((win_w - self.offset[0]) / self.cell_size).ceil() as isize;
        let min_y = ((-self.offset[1]) / self.cell_size).floor() as isize;
        let max_y = ((win_h - self.offset[1]) / self.cell_size).ceil() as isize;
        (min_x, max_x, min_y, max_y)
    }
}

const SCREEN_WIDTH: u32 = 960;
const SCREEN_HEIGHT: u32 = 720; 

pub fn run(state_rx: Receiver<Snapshot>, cmd_tx: Sender<SimCommand>) {
    let presets = Pattern::presets();
    let mut active_pattern: Option<Pattern> = None;
    let mut shift_held = false;

    let mut window: PistonWindow = WindowSettings::new(
        "Game of Life",
        [SCREEN_WIDTH, SCREEN_HEIGHT],
    )
    .exit_on_esc(true)
    .build()
    .expect("无法创建 Piston 窗口");

    let mut snapshot = Snapshot {
        cells: Arc::new(HashSet::new()),
        generation: 0,
        paused: false,
        rule: Rule::CONWAY,
        interval_ms: 60
    };

    let mut camera = Camera::new();
    let mut cursor_pos = [0.0, 0.0];

    // 交互状态机
    let mut is_panning = false;
    let mut paint_brush: Option<bool> = None; // Some(true)=绘制, Some(false)=擦除
    let mut last_painted: Option<Coord> = None;
    let mut hud_dirty = true; // 仅在数据变化时更新窗口标题，避免无谓的系统调用

    // event loop
    while let Some(event) = window.next() {
        // 1. 拉取后台最新快照
        if let Ok(latest) = state_rx.try_recv() {
            snapshot = latest;
            hud_dirty = true;
        }

        if let Some(pos) = event.mouse_cursor_args() {
            if is_panning {
                camera.offset[0] += pos[0] - cursor_pos[0];
                camera.offset[1] += pos[1] - cursor_pos[1];
            }
            
            if let Some(target_alive) = paint_brush {
                let coord = camera.screen_to_world(pos);
                if last_painted != Some(coord) {
                    let _ = cmd_tx.send(SimCommand::SetCell(coord, target_alive));
                    last_painted = Some(coord);
                }
            }

            cursor_pos = pos;
        }

        // 0. 监听系统文件拖拽（直接把 .rle 文件拖进窗口即可加载）
        if let Event::Input(Input::FileDrag(FileDrag::Drop(path)), _) = &event {
            match Pattern::from_file(path) {
                Ok(pat) => {
                    active_pattern = Some(pat);
                    paint_brush = None;
                    hud_dirty = true;
                }
                Err(err) => eprintln!("导入 RLE 失败: {}", err),
            }
        }

        // 记录 Shift 键状态（支持按住 Shift 连续盖章）
        if let Some(Button::Keyboard(Key::LShift | Key::RShift)) = event.press_args() {
            shift_held = true;
        }
        if let Some(Button::Keyboard(Key::LShift | Key::RShift)) = event.release_args() {
            shift_held = false;
        }

        // 3. 处理鼠标滚轮缩放
        if let Some(scroll) = event.mouse_scroll_args() {
            if scroll[1] != 0.0 {
                let factor = if scroll[1] > 0.0 { 1.15 } else { 1.0 / 1.15 };
                camera.zoom_at(cursor_pos, factor);
                hud_dirty = true;
            }
        }

        // 4. 处理按键/鼠标按下
        if let Some(button) = event.press_args() {
            match button {
                Button::Mouse(MouseButton::Left) => {
                    let coord = camera.screen_to_world(cursor_pos);
                    if let Some(pat) = &active_pattern {
                        // 处于图样印章模式：批量放置图样
                        let _ = cmd_tx.send(SimCommand::PlacePattern(coord, Arc::clone(&pat.cells)));
                        if !shift_held {
                            active_pattern = None;
                            hud_dirty = true;
                        }
                    } else {
                        // 普通绘制/擦除模式
                        let target_alive = !snapshot.cells.contains(&coord);
                        paint_brush = Some(target_alive);
                        last_painted = Some(coord);
                        let _ = cmd_tx.send(SimCommand::SetCell(coord, target_alive));
                    }
                }
                Button::Mouse(MouseButton::Right) => {
                    if active_pattern.is_some() {
                        // 右键优先取消当前图样预览
                        active_pattern = None;
                        hud_dirty = true;
                    } else {
                        is_panning = true;
                    }
                }
                Button::Mouse(MouseButton::Middle) => {
                    is_panning = true;
                }
                // 数字键 1~5 选择预设图样
                Button::Keyboard(key @ (Key::D1 | Key::D2 | Key::D3 | Key::D4 | Key::D5)) => {
                    let idx = match key {
                        Key::D1 => 0,
                        Key::D2 => 1,
                        Key::D3 => 2,
                        Key::D4 => 3,
                        Key::D5 => 4,
                        _ => 0,
                    };
                    if let Some(pat) = presets.get(idx) {
                        active_pattern = Some(pat.clone());
                        paint_brush = None;
                        hud_dirty = true;
                    }
                }
                // R 旋转 / F 翻转 / Backspace 取消图样
                Button::Keyboard(Key::R) => {
                    if let Some(pat) = &mut active_pattern {
                        pat.rotate_cw();
                    }
                }
                Button::Keyboard(Key::F) => {
                    if let Some(pat) = &mut active_pattern {
                        pat.flip_horizontal();
                    }
                }
                Button::Keyboard(Key::Backspace) => {
                    if active_pattern.take().is_some() {
                        hud_dirty = true;
                    }
                }
                // F1~F4
                Button::Keyboard(Key::F1) => {
                    let _ = cmd_tx.send(SimCommand::SetRule(Rule::CONWAY));
                }
                Button::Keyboard(Key::F2) => {
                    let _ = cmd_tx.send(SimCommand::SetRule(Rule::HIGH_LIFE));
                }
                Button::Keyboard(Key::F3) => {
                    let _ = cmd_tx.send(SimCommand::SetRule(Rule::SEEDS));
                }
                Button::Keyboard(Key::F4) => {
                    let _ = cmd_tx.send(SimCommand::SetRule(Rule::DAY_AND_NIGHT));
                }

                Button::Keyboard(Key::Space) => {
                    let _ = cmd_tx.send(SimCommand::TogglePause);
                }
                Button::Keyboard(Key::C) => {
                    let _ = cmd_tx.send(SimCommand::Clear);
                }
                Button::Keyboard(Key::Home) => {
                    camera.reset();
                    hud_dirty = true;
                }
                // 1. 键盘输入处理（在 match button 中补充）：
                Button::Keyboard(Key::N | Key::Right) => {
                    let _ = cmd_tx.send(SimCommand::Step);
                }
                Button::Keyboard(Key::Z | Key::Left) => {
                    let _ = cmd_tx.send(SimCommand::Rewind);
                }
                Button::Keyboard(Key::Equals | Key::Plus | Key::Up) => {
                    // 间隔缩短为 0.75 倍 -> 速度提升
                    let _ = cmd_tx.send(SimCommand::AdjustSpeed(0.75));
                }
                Button::Keyboard(Key::Minus | Key::Down) => {
                    // 间隔延长为 1.33 倍 -> 速度放慢
                    let _ = cmd_tx.send(SimCommand::AdjustSpeed(1.33));
                }
                _ => {}
            }
        }

        // 5. 处理鼠标松开（结束拖拽或涂抹）
        if let Some(button) = event.release_args() {
            match button {
                Button::Mouse(MouseButton::Left) => {
                    paint_brush = None;
                    last_painted = None;
                }
                Button::Mouse(MouseButton::Right | MouseButton::Middle) => {
                    is_panning = false;
                }
                _ => {}
            }
        }

        // 6. 按需更新窗口标题栏 HUD
        if hud_dirty {
            let status = if snapshot.paused { "PAUSED" } else { "RUNNING" };
            let tps = 1000.0 / (snapshot.interval_ms.max(1) as f64);
            let stamp_info = match &active_pattern {
                Some(pat) => format!(" | Stamp: {} [R:旋转 F:翻转 左键:放置 右键:取消]", pat.name),
                None => String::new(),
            };
            let title = format!(
                "Game of Life [{}] | Rule: {} | Gen: {} | Pop: {} | {:.1} TPS{}",
                status,
                snapshot.rule.name,
                snapshot.generation,
                snapshot.cells.len(),
                tps,
                stamp_info
            );
            window.set_title(title);
            hud_dirty = false;
        }

        // 7. 渲染视野内的网格与细胞
        let Size { width: win_w, height: win_h } = window.size();
            window.draw_2d(&event, |context, graphics, _device| {
            clear([0.11, 0.12, 0.14, 1.0], graphics);

            draw_grid(&camera, win_w, win_h, &context, graphics);
            draw_cells(&camera, &snapshot.cells, win_w, win_h, &context, graphics);

            // 如果当前选中了图样印章，在光标处绘制半透明预览
            if let Some(pat) = &active_pattern {
                let hover_coord = camera.screen_to_world(cursor_pos);
                draw_ghost_pattern(&camera, hover_coord, &pat.cells, &context, graphics);
            }
        });
    }

    fn draw_grid<G: Graphics>(camera: &Camera, win_w: f64, win_h: f64, context: &Context, graphics: &mut G){
        // 缩得太小时隐藏网格线，提升视觉清晰度与渲染性能
        if camera.cell_size < 6.0 {
            return;
        }

        let (min_x, max_x, min_y, max_y) = camera.visible_bounds(win_w, win_h);
        let grid_color = [0.16, 0.17, 0.20, 1.0];
        let axis_color = [0.25, 0.27, 0.32, 1.0]; // 原点坐标轴略微高亮，方便辨别方位
        
        for wx in min_x..=max_x {
            let [px, _] = camera.world_to_screen((wx, 0));
            let color = if wx == 0 { axis_color } else { grid_color };
            line(color, 0.5, [px, 0.0, px, win_h], context.transform, graphics);
        }

        for wy in min_y..=max_y {
            let [_, py] = camera.world_to_screen((0, wy));
            let color = if wy == 0 { axis_color } else { grid_color };
            line(color, 0.5, [0.0, py, win_w, py], context.transform, graphics);
        }
    }

    fn draw_cells<G: Graphics>(camera: &Camera, cells: &HashSet<Coord>, win_w: f64, win_h: f64, context: &Context, graphics: &mut G) {
        let (min_x, max_x, min_y, max_y) = camera.visible_bounds(win_w, win_h);
        let cell_color = [0.22, 0.85, 0.55, 1.0];
        // 格子较小时去掉内边距间隙，避免细胞缩成细点
        let inset = if camera.cell_size >= 8.0 { 1.0 } else { 0.0 };
        let size = (camera.cell_size - inset * 2.0).max(1.0);

        for &(wx, wy) in cells {
            if (min_x..=max_x).contains(&wx) && (min_y..=max_y).contains(&wy) {
                let [px, py] = camera.world_to_screen((wx, wy));
                rectangle(
                    cell_color,
                    [px + inset, py + inset, size, size],
                    context.transform,
                    graphics,
                );
            }
        }
    }

    fn draw_ghost_pattern<G: Graphics>(camera: &Camera, (ox, oy): Coord, cells: &[Coord], context: &Context, graphics: &mut G){
        // 琥珀色半透明幽灵预览（RGBA 最后一个分量 0.45 为透明度）
        let ghost_color = [0.95, 0.75, 0.25, 0.45];
        let inset = if camera.cell_size >= 8.0 { 1.0 } else { 0.0 };
        let size = (camera.cell_size - inset * 2.0).max(1.0);

        for &(dx, dy) in cells {
            let [px, py] = camera.world_to_screen((ox + dx, oy + dy));
            rectangle(ghost_color, [px + inset, py + inset, size, size], context.transform, graphics);
        }
    }
}