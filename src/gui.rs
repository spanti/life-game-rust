use crate::pattern::Pattern;
use crate::{
    sim::SimCommand,
    universe::{Coord, Rule, Snapshot},
};
use crossbeam::channel::{Receiver, Sender};
use piston_window::{Button::Keyboard, *};
use std::{collections::HashSet, sync::Arc};

const MIN_CELL_SIZE: f64 = 2.0;
const MAX_CELL_SIZE: f64 = 64.0;
const DEFAULT_CELL_SIZE: f64 = 16.0;

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SymmetryMode {
    None,
    Horizontal, // 左右镜像（关于 X=0 轴对称）
    Quad,       // 四象限中心镜像（关于 X=0 和 Y=0 双轴对称）
}

impl SymmetryMode {
    fn next(self) -> Self {
        match self {
            Self::None => Self::Horizontal,
            Self::Horizontal => Self::Quad,
            Self::Quad => Self::None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "Off",
            Self::Horizontal => "Mirror-X",
            Self::Quad => "Quad-XY",
        }
    }

    /// 根据当前对称模式展开单个坐标为对称坐标组
    fn expand(self, (x, y): Coord) -> Vec<Coord> {
        match self {
            Self::None => vec![(x, y)],
            Self::Horizontal => vec![(x, y), (-x - 1, y)], // 关于 X=0 网格线对称
            Self::Quad => vec![(x, y), (-x - 1, y), (x, -y - 1), (-x - 1, -y - 1)],
        }
    }
}

/// 在指定中心点生成 32x32 随机汤（~32% 填充率，零外部依赖 SplitMix64 算法）
fn generate_random_soup((cx, cy): Coord) -> Vec<Coord> {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x1234_5678_9abc_def0);

    let mut next_rand = || -> u64 {
        seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };

    let half = 16isize;
    let mut cells = Vec::with_capacity(350);
    for dy in -half..half {
        for dx in -half..half {
            // 约 32% 概率存活
            if (next_rand() % 100) < 32 {
                cells.push((cx + dx, cy + dy));
            }
        }
    }

    cells
}

// 2d camera
struct Camera {
    offset: [f64; 2],
    cell_size: f64,
}

impl Camera {
    fn new() -> Self {
        Self {
            offset: [100.0, 100.0],
            cell_size: DEFAULT_CELL_SIZE,
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

    fn zoom_at(&mut self, cursor: [f64; 2], factor: f64) {
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

pub struct App {
    window: PistonWindow,
    state_rx: Receiver<Snapshot>,
    cmd_tx: Sender<SimCommand>,
    presets: Vec<Pattern>,
    snapshot: Snapshot,
    camera: Camera,
    cursor_pos: [f64; 2],
    active_pattern: Option<Pattern>,
    shift_held: bool,
    ctrl_held: bool,
    symmetry: SymmetryMode,
    selection: Option<(Coord, Coord)>,
    is_panning: bool,
    is_selecting: bool,
    paint_brush: Option<bool>,
    last_painted: Option<Coord>,
    status_toast: Option<String>,
    hud_dirty: bool,
}

impl App {
    pub fn new(state_rx: Receiver<Snapshot>, cmd_tx: Sender<SimCommand>) -> Self {
        let window: PistonWindow =
            WindowSettings::new("Game of Life", [SCREEN_WIDTH, SCREEN_HEIGHT])
                .exit_on_esc(true)
                .build()
                .expect("无法创建 Piston 窗口");

        let snapshot = Snapshot {
            cells: Arc::new(HashSet::new()),
            generation: 0,
            paused: false,
            rule: Rule::CONWAY,
            interval_ms: 60,
        };

        Self {
            window,
            state_rx,
            cmd_tx,
            presets: Pattern::presets(),
            snapshot,
            camera: Camera::new(),
            cursor_pos: [0.0, 0.0],
            active_pattern: None,
            shift_held: false,
            ctrl_held: false,
            symmetry: SymmetryMode::None,
            selection: None,
            is_panning: false,
            is_selecting: false,
            paint_brush: None,
            last_painted: None,
            status_toast: None,
            hud_dirty: true,
        }
    }

    pub fn run(mut self) {
        while let Some(event) = self.window.next() {
            self.handle_event(&event);
            self.update_title();
            self.draw(&event);
        }
    }

    fn handle_event(&mut self, event: &Event) {
        // 1. 拉取后台最新快照
        if let Ok(latest) = self.state_rx.try_recv() {
            self.snapshot = latest;
            self.hud_dirty = true;
        }

        self.handle_mouse_cursor(event);
        self.handle_file_drop(event);
        self.handle_scroll(event);

        if let Some(button) = event.press_args() {
            self.handle_press(button);
        }

        if let Some(button) = event.release_args() {
            self.handle_release(button);
        }
    }

    fn handle_mouse_cursor(&mut self, event: &Event) {
        let Some(pos) = event.mouse_cursor_args() else {
            return;
        };

        // 鼠标移动时更新框选区域或对称涂抹
        if self.is_panning {
            self.camera.offset[0] += pos[0] - self.cursor_pos[0];
            self.camera.offset[1] += pos[1] - self.cursor_pos[1];
        } else if self.is_selecting {
            if let Some((start, _)) = self.selection {
                self.selection = Some((start, self.camera.screen_to_world(pos)));
            }
        } else if let Some(target_alive) = self.paint_brush {
            let coord = self.camera.screen_to_world(pos);
            if self.last_painted != Some(coord) {
                let expanded = self.symmetry.expand(coord);
                let _ = self
                    .cmd_tx
                    .send(SimCommand::SetCells(expanded, target_alive));
                self.last_painted = Some(coord);
            }
        }

        self.cursor_pos = pos;
    }

    fn handle_file_drop(&mut self, event: &Event) {
        // 监听系统文件拖拽（直接把 .rle 文件拖进窗口即可加载）
        if let Event::Input(Input::FileDrag(FileDrag::Drop(path)), _) = event {
            match Pattern::from_file(path) {
                Ok(pat) => {
                    self.active_pattern = Some(pat);
                    self.paint_brush = None;
                    self.hud_dirty = true;
                }
                Err(err) => eprintln!("导入 RLE 失败: {}", err),
            }
        }
    }


    fn handle_scroll(&mut self, event: &Event) {
        if let Some(scroll) = event.mouse_scroll_args() {
            if scroll[1] != 0.0 {
                let factor = if scroll[1] > 0.0 { 1.15 } else { 1.0 / 1.15 };
                self.camera.zoom_at(self.cursor_pos, factor);
                self.hud_dirty = true;
            }
        }
    }

    fn handle_press(&mut self, button: Button) {
        match button {
            // 记录 Shift 键状态（支持按住 Shift 连续盖章）
            Button::Keyboard(Key::LShift | Key::RShift) => {
                self.shift_held = true;
            }

            // 监听 Ctrl 键状态
            Button::Keyboard(Key::LCtrl | Key::RCtrl) => {
                self.ctrl_held = true;
            }

            Button::Mouse(MouseButton::Left) => {
                let coord = self.camera.screen_to_world(self.cursor_pos);
                if self.ctrl_held {
                    // Ctrl + 左键：开启区域框选
                    self.selection = Some((coord, coord));
                    self.is_selecting = true;
                    self.active_pattern = None;
                    self.paint_brush = None;
                } else if let Some(pat) = &self.active_pattern {
                    // 处于图样印章模式：批量放置图样
                    let _ = self
                        .cmd_tx
                        .send(SimCommand::PlacePattern(coord, Arc::clone(&pat.cells)));
                    if !self.shift_held {
                        self.active_pattern = None;
                        self.hud_dirty = true;
                    }
                } else {
                    // 对称/普通画笔落子（同时清除旧框选）
                    self.selection = None;
                    let target_alive = !self.snapshot.cells.contains(&coord);
                    self.paint_brush = Some(target_alive);
                    self.last_painted = Some(coord);
                    let expanded = self.symmetry.expand(coord);
                    let _ = self
                        .cmd_tx
                        .send(SimCommand::SetCells(expanded, target_alive));
                }
            }
            Button::Mouse(MouseButton::Right) => {
                if self.active_pattern.take().is_some() || self.selection.take().is_some() {
                    self.hud_dirty = true;
                } else {
                    self.is_panning = true;
                }
            }
            // M 键：切换对称绘制模式
            Button::Keyboard(Key::M) => {
                self.symmetry = self.symmetry.next();
                self.hud_dirty = true;
            }
            // G 键：在当前屏幕中心生成 32x32 随机汤
            Button::Keyboard(Key::G) => {
                let Size { width, height } = self.window.size();
                let center = self.camera.screen_to_world([width / 2.0, height / 2.0]);
                let soup = generate_random_soup(center);
                let _ = self.cmd_tx.send(SimCommand::SetCells(soup, true));
                self.status_toast = Some("Generated 32x32 Random Soup".into());
                self.hud_dirty = true;
            }
            // Ctrl + C：将框选区域提取为图样印章
            Button::Keyboard(Key::C) if self.ctrl_held => {
                if let Some((p1, p2)) = self.selection {
                    let (min_x, max_x) = (p1.0.min(p2.0), p1.0.max(p2.0));
                    let (min_y, max_y) = (p1.1.min(p2.1), p1.1.max(p2.1));
                    let selected_cells: Vec<Coord> = self
                        .snapshot
                        .cells
                        .iter()
                        .copied()
                        .filter(|&(x, y)| {
                            (min_x..=max_x).contains(&x) && (min_y..=max_y).contains(&y)
                        })
                        .collect();

                    match Pattern::from_slice("Copied Selection", &selected_cells) {
                        Ok(pat) => {
                            self.active_pattern = Some(pat);
                            self.selection = None;
                            self.status_toast =
                                Some(format!("Copied {} cells to stamp", selected_cells.len()));
                            self.hud_dirty = true;
                        }
                        Err(msg) => {
                            self.status_toast =
                                Some(format!("Copied {} cells to stamp", selected_cells.len()));
                            self.hud_dirty = true;
                        }
                    }
                }
            }
            // 单独按 C（未按 Ctrl）：清空画布
            Button::Keyboard(Key::C) if !self.ctrl_held => {
                self.selection = None;
                let _ = self.cmd_tx.send(SimCommand::Clear);
            }
            // Ctrl + S：导出框选区域（若无框选则导出整个世界）为 .rle 文件
            Button::Keyboard(Key::S) if self.ctrl_held => {
                let export_cells: Vec<Coord> = if let Some((p1, p2)) = self.selection {
                    let (min_x, max_x) = (p1.0.min(p2.0), p1.0.max(p2.0));
                    let (min_y, max_y) = (p1.1.min(p2.1), p1.1.max(p2.1));
                    self.snapshot
                        .cells
                        .iter()
                        .copied()
                        .filter(|&(x, y)| {
                            (min_x..=max_x).contains(&x) && (min_y..=max_y).contains(&y)
                        })
                        .collect()
                } else {
                    self.snapshot.cells.iter().copied().collect()
                };

                match Pattern::from_slice("Exported Pattern", &export_cells) {
                    Ok(pat) => {
                        match pat.save_to_timestamped_file(self.snapshot.rule.notation) {
                            Ok(filename) => {
                                self.status_toast = Some(format!("Saved: {}", filename))
                            }
                            Err(e) => self.status_toast = Some(e),
                        }
                        self.hud_dirty = true;
                    }
                    Err(msg) => {
                        self.status_toast = Some(format!("Exported failed: {msg}"));
                    }
                }
            }
            Button::Mouse(MouseButton::Middle) => {
                self.is_panning = true;
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
                if let Some(pat) = self.presets.get(idx) {
                    self.active_pattern = Some(pat.clone());
                    self.paint_brush = None;
                    self.hud_dirty = true;
                }
            }
            // R 旋转 / F 翻转 / Backspace 取消图样
            Button::Keyboard(Key::R) => {
                if let Some(pat) = &mut self.active_pattern {
                    pat.rotate_cw();
                }
            }
            Button::Keyboard(Key::F) => {
                if let Some(pat) = &mut self.active_pattern {
                    pat.flip_horizontal();
                }
            }
            Button::Keyboard(Key::Backspace) => {
                if self.active_pattern.take().is_some() {
                    self.hud_dirty = true;
                }
            }
            // F1~F4
            Button::Keyboard(Key::F1) => {
                let _ = self.cmd_tx.send(SimCommand::SetRule(Rule::CONWAY));
            }
            Button::Keyboard(Key::F2) => {
                let _ = self.cmd_tx.send(SimCommand::SetRule(Rule::HIGH_LIFE));
            }
            Button::Keyboard(Key::F3) => {
                let _ = self.cmd_tx.send(SimCommand::SetRule(Rule::SEEDS));
            }
            Button::Keyboard(Key::F4) => {
                let _ = self.cmd_tx.send(SimCommand::SetRule(Rule::DAY_AND_NIGHT));
            }
            Button::Keyboard(Key::Space) => {
                let _ = self.cmd_tx.send(SimCommand::TogglePause);
                self.status_toast = None;
            }
            Button::Keyboard(Key::Home) => {
                self.camera.reset();
                self.hud_dirty = true;
            }
            // 1. 键盘输入处理（在 match button 中补充）：
            Button::Keyboard(Key::N | Key::Right) => {
                let _ = self.cmd_tx.send(SimCommand::Step);
            }
            Button::Keyboard(Key::Z | Key::Left) => {
                let _ = self.cmd_tx.send(SimCommand::Rewind);
            }
            Button::Keyboard(Key::Equals | Key::Plus | Key::Up) => {
                // 间隔缩短为 0.75 倍 -> 速度提升
                let _ = self.cmd_tx.send(SimCommand::AdjustSpeed(0.75));
            }
            Button::Keyboard(Key::Minus | Key::Down) => {
                // 间隔延长为 1.33 倍 -> 速度放慢
                let _ = self.cmd_tx.send(SimCommand::AdjustSpeed(1.33));
            }
            _ => {}
        }
    }

    fn handle_release(&mut self, button: Button) {
        match button {
            Button::Keyboard(Key::LShift | Key::RShift) => {
                self.shift_held = false;
            }

            Button::Keyboard(Key::LCtrl | Key::RCtrl) => {
                self.ctrl_held = false;
            }

            Button::Mouse(MouseButton::Left) => {
                self.paint_brush = None;
                self.last_painted = None;
                self.is_selecting = false;
            }
            Button::Mouse(MouseButton::Right | MouseButton::Middle) => {
                self.is_panning = false;
            }
            _ => {}
        }
    }

    fn update_title(&mut self) {
        if !self.hud_dirty {
            return;
        }

        let status = if self.snapshot.paused {
            "PAUSED"
        } else {
            "RUNNING"
        };
        let tps = 1000.0 / (self.snapshot.interval_ms.max(1) as f64);

        // 对称模式提示（仅在非 None 时显示）
        let sym_info = match self.symmetry {
            SymmetryMode::None => String::new(),
            mode => format!(" | Sym: {}", mode.label()),
        };

        // 图样印章提示
        let stamp_info = match &self.active_pattern {
            Some(pat) => format!(" | Stamp: {} [R:旋转 F:翻转 左键:放置 右键:取消]", pat.name),
            None => String::new(),
        };

        // 临时状态反馈提示（如导出文件名、生成随机汤等）
        let toast_info = match &self.status_toast {
            Some(msg) => format!(" | ★ {}", msg),
            None => String::new(),
        };

        let title = format!(
            "Game of Life [{}] | {} | Gen: {} | Pop: {} | {:.1} TPS{}{}{}",
            status,
            self.snapshot.rule.name,
            self.snapshot.generation,
            self.snapshot.cells.len(),
            tps,
            sym_info,
            stamp_info,
            toast_info
        );

        self.window.set_title(title);
        self.hud_dirty = false;
    }

    fn draw(&mut self, event: &Event) {
        let Size {
            width: win_w,
            height: win_h,
        } = self.window.size();
        let camera = &self.camera;
        let snapshot = &self.snapshot;
        let active_pattern = &self.active_pattern;
        let cursor_pos = self.cursor_pos;
        let symmetry = self.symmetry;
        let selection = self.selection;

        self.window.draw_2d(event, |context, graphics, _device| {
            // 1. 最底层：深灰背景清屏
            clear([0.11, 0.12, 0.14, 1.0], graphics);

            // 2. 背景网格与原点坐标轴
            draw_grid(camera, win_w, win_h, &context, graphics);

            // 3. 对称模式辅助线（画在网格之上、细胞之下，避免遮挡细胞）
            draw_symmetry_axes(camera, symmetry, win_w, win_h, &context, graphics);

            // 4. 视野内的所有存活细胞
            draw_cells(camera, &snapshot.cells, win_w, win_h, &context, graphics);

            // 5. 图样印章半透明幽灵预览（若处于盖章模式）
            if let Some(pat) = active_pattern {
                let hover_coord = camera.screen_to_world(cursor_pos);
                draw_ghost_pattern(camera, hover_coord, &pat.cells, &context, graphics);
            }

            // 6. 最顶层：Ctrl + 拖拽区域框选高亮框
            if let Some(sel) = selection {
                draw_selection_box(camera, sel, &context, graphics);
            }
        });
    }
}

fn draw_grid<G: Graphics>(
    camera: &Camera,
    win_w: f64,
    win_h: f64,
    context: &Context,
    graphics: &mut G,
) {
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
        line(
            color,
            0.5,
            [px, 0.0, px, win_h],
            context.transform,
            graphics,
        );
    }

    for wy in min_y..=max_y {
        let [_, py] = camera.world_to_screen((0, wy));
        let color = if wy == 0 { axis_color } else { grid_color };
        line(
            color,
            0.5,
            [0.0, py, win_w, py],
            context.transform,
            graphics,
        );
    }
}

fn draw_cells<G: Graphics>(
    camera: &Camera,
    cells: &HashSet<Coord>,
    win_w: f64,
    win_h: f64,
    context: &Context,
    graphics: &mut G,
) {
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

fn draw_ghost_pattern<G: Graphics>(
    camera: &Camera,
    (ox, oy): Coord,
    cells: &[Coord],
    context: &Context,
    graphics: &mut G,
) {
    // 琥珀色半透明幽灵预览（RGBA 最后一个分量 0.45 为透明度）
    let ghost_color = [0.95, 0.75, 0.25, 0.45];
    let inset = if camera.cell_size >= 8.0 { 1.0 } else { 0.0 };
    let size = (camera.cell_size - inset * 2.0).max(1.0);

    for &(dx, dy) in cells {
        let [px, py] = camera.world_to_screen((ox + dx, oy + dy));
        rectangle(
            ghost_color,
            [px + inset, py + inset, size, size],
            context.transform,
            graphics,
        );
    }
}

fn draw_symmetry_axes<G: Graphics>(
    camera: &Camera,
    symmetry: SymmetryMode,
    win_w: f64,
    win_h: f64,
    context: &Context,
    graphics: &mut G,
) {
    if symmetry == SymmetryMode::None {
        return;
    }

    let sym_color = [0.85, 0.35, 0.65, 0.65]; // 品红色对称指示线
    let [ox, oy] = camera.world_to_screen((0, 0));

    // 垂直对称轴 (X = 0)
    line(
        sym_color,
        1.5,
        [ox, 0.0, ox, win_h],
        context.transform,
        graphics,
    );

    // 若为四象限对称，额外绘制水平对称轴 (Y = 0)
    if symmetry == SymmetryMode::Quad {
        line(
            sym_color,
            1.5,
            [0.0, oy, win_w, oy],
            context.transform,
            graphics,
        );
    }
}

fn draw_selection_box<G: Graphics>(
    camera: &Camera,
    (p1, p2): (Coord, Coord),
    context: &Context,
    graphics: &mut G,
) {
    let min_x = p1.0.min(p2.0);
    let max_x = p1.0.max(p2.0) + 1;
    let min_y = p1.1.min(p2.1);
    let max_y = p1.1.max(p2.1) + 1;

    let [x0, y0] = camera.world_to_screen((min_x, min_y));
    let [x1, y1] = camera.world_to_screen((max_x, max_y));
    let w = x1 - x0;
    let h = y1 - y0;

    // 半透明蓝色填充 + 边框
    rectangle(
        [0.25, 0.60, 0.95, 0.20],
        [x0, y0, w, h],
        context.transform,
        graphics,
    );
    rectangle::Rectangle::new_border([0.35, 0.70, 1.0, 0.85], 1.0).draw(
        [x0, y0, w, h],
        &context.draw_state,
        context.transform,
        graphics,
    );
}
