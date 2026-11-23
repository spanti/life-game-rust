use crate::keymap::KeyRegistry;
use crate::pattern::Pattern;
use crate::{
    sim::SimCommand,
    universe::{Coord, Rule, Snapshot},
};
use crossbeam::channel::{Receiver, Sender};
use eframe::egui::{
    self, Color32, Context, Event, Key, Modifiers, Painter, PointerButton, Pos2, Rect, Sense,
    Stroke, StrokeKind, ViewportCommand, pos2, vec2,
};
use std::{
    cell::Cell,
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MIN_CELL_SIZE: f64 = 2.0;
const MAX_CELL_SIZE: f64 = 64.0;
const DEFAULT_CELL_SIZE: f64 = 16.0;
const SCREEN_WIDTH: f32 = 960.0;
const SCREEN_HEIGHT: f32 = 720.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SymmetryMode {
    None,
    Horizontal,
    Quad,
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

    fn expand(self, (x, y): Coord) -> Vec<Coord> {
        match self {
            Self::None => vec![(x, y)],
            Self::Horizontal => vec![(x, y), (-x - 1, y)],
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

    (-16..16)
        .flat_map(|dy| (-16..16).map(move |dx| (cx + dx, cy + dy)))
        .filter(|_| (next_rand() % 100) < 32)
        .collect()
}

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

    fn screen_to_world(&self, p: Pos2) -> Coord {
        let wx = ((f64::from(p.x) - self.offset[0]) / self.cell_size).floor() as isize;
        let wy = ((f64::from(p.y) - self.offset[1]) / self.cell_size).floor() as isize;
        (wx, wy)
    }

    fn world_to_screen(&self, (wx, wy): Coord) -> Pos2 {
        pos2(
            (wx as f64 * self.cell_size + self.offset[0]) as f32,
            (wy as f64 * self.cell_size + self.offset[1]) as f32,
        )
    }

    fn zoom_at(&mut self, cursor: Pos2, factor: f64) {
        let new_size = (self.cell_size * factor).clamp(MIN_CELL_SIZE, MAX_CELL_SIZE);
        let ratio = new_size / self.cell_size;
        let cursor = [f64::from(cursor.x), f64::from(cursor.y)];
        self.offset[0] = cursor[0] - (cursor[0] - self.offset[0]) * ratio;
        self.offset[1] = cursor[1] - (cursor[1] - self.offset[1]) * ratio;
        self.cell_size = new_size;
    }

    fn visible_bounds(&self, rect: Rect) -> (isize, isize, isize, isize) {
        let (min_x, min_y) = self.screen_to_world(rect.min);
        let (max_x, max_y) = self.screen_to_world(rect.max);
        (min_x, max_x + 1, min_y, max_y + 1)
    }
}

pub struct LifeApp {
    state_rx: Receiver<Snapshot>,
    cmd_tx: Sender<SimCommand>,
    presets: Vec<Pattern>,
    snapshot: Snapshot,
    camera: Camera,
    cursor_pos: Pos2,
    active_pattern: Option<Pattern>,
    symmetry: SymmetryMode,
    selection: Option<(Coord, Coord)>,
    is_panning: bool,
    is_selecting: bool,
    paint_brush: Option<bool>,
    last_painted: Option<Coord>,
    status_toast: Option<String>,
    status_text: String,
    hud_dirty: bool,
    help_open: bool,
    help_text: Option<String>,
    help_rect: Option<Rect>,
}

impl LifeApp {
    pub fn new(state_rx: Receiver<Snapshot>, cmd_tx: Sender<SimCommand>) -> Self {
        let snapshot = Snapshot {
            cells: Arc::new(HashSet::new()),
            generation: 0,
            paused: false,
            rule: Rule::CONWAY,
            interval_ms: 60,
        };

        Self {
            state_rx,
            cmd_tx,
            presets: Pattern::presets(),
            snapshot,
            camera: Camera::new(),
            cursor_pos: Pos2::ZERO,
            active_pattern: None,
            symmetry: SymmetryMode::None,
            selection: None,
            is_panning: false,
            is_selecting: false,
            paint_brush: None,
            last_painted: None,
            status_toast: None,
            status_text: String::new(),
            hud_dirty: true,
            help_open: false,
            help_text: None,
            help_rect: None,
        }
    }

    pub fn run(self) {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([SCREEN_WIDTH, SCREEN_HEIGHT]),
            ..Default::default()
        };

        eframe::run_native(
            "Game of Life",
            options,
            Box::new(move |cc| {
                setup_chinese_font(&cc.egui_ctx);
                Ok(Box::new(self))
            }),
        )
        .expect("无法创建 egui 窗口");
    }

    fn send_command(&self, cmd: SimCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn selection_cells(&self, fallback_all: bool) -> Option<Vec<Coord>> {
        if let Some((p1, p2)) = self.selection {
            let (x0, x1) = (p1.0.min(p2.0), p1.0.max(p2.0));
            let (y0, y1) = (p1.1.min(p2.1), p1.1.max(p2.1));
            Some(
                self.snapshot
                    .cells
                    .iter()
                    .copied()
                    .filter(|&(x, y)| (x0..=x1).contains(&x) && (y0..=y1).contains(&y))
                    .collect(),
            )
        } else if fallback_all {
            Some(self.snapshot.cells.iter().copied().collect())
        } else {
            None
        }
    }

    fn handle_file_drop(&mut self, ctx: &Context) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
        for file in dropped_files {
            match Pattern::from_file(file.path()) {
                Ok(pat) => {
                    self.active_pattern = Some(pat);
                    self.paint_brush = None;
                    self.hud_dirty = true;
                }
                Err(err) => eprintln!("导入 RLE 失败: {err}"),
            }
        }
    }

    fn pointer_over_help(&self, pos: Pos2) -> bool {
        self.help_open && self.help_rect.is_some_and(|rect| rect.contains(pos))
    }

    fn handle_events(&mut self, ctx: &Context, rect: Rect) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                Event::Copy => self.handle_key(Key::C, true, rect.center()),
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if key == Key::Escape {
                        if self.help_open {
                            self.help_open = false;
                        } else {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    } else {
                        self.handle_key(key, modifiers.ctrl, rect.center());
                    }
                }
                Event::PointerMoved(pos) => {
                    if self.pointer_over_help(pos) {
                        self.cursor_pos = pos;
                    } else {
                        self.handle_mouse_cursor(pos);
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    if pressed {
                        if self.pointer_over_help(pos) {
                            self.cursor_pos = pos;
                        } else if rect.contains(pos) {
                            self.handle_press(button, pos, modifiers);
                        }
                    } else {
                        self.handle_release(button);
                    }
                }
                Event::MouseWheel { delta, .. }
                    if delta.y != 0.0
                        && rect.contains(self.cursor_pos)
                        && !self.pointer_over_help(self.cursor_pos) =>
                {
                    self.handle_scroll(delta.y);
                }
                _ => {}
            }
        }
    }

    fn handle_mouse_cursor(&mut self, pos: Pos2) {
        if self.is_panning {
            self.camera.offset[0] += f64::from(pos.x - self.cursor_pos.x);
            self.camera.offset[1] += f64::from(pos.y - self.cursor_pos.y);
        } else if self.is_selecting {
            if let Some((start, _)) = self.selection {
                self.selection = Some((start, self.camera.screen_to_world(pos)));
            }
        } else if let Some(target_alive) = self.paint_brush {
            let coord = self.camera.screen_to_world(pos);
            if self.last_painted != Some(coord) {
                let expanded = self.symmetry.expand(coord);
                self.send_command(SimCommand::SetCells(expanded, target_alive));
                self.last_painted = Some(coord);
            }
        }

        self.cursor_pos = pos;
    }

    fn handle_scroll(&mut self, scroll_y: f32) {
        let factor = if scroll_y > 0.0 { 1.15 } else { 1.0 / 1.15 };
        self.camera.zoom_at(self.cursor_pos, factor);
        self.hud_dirty = true;
    }

    fn handle_press(&mut self, button: PointerButton, pos: Pos2, modifiers: Modifiers) {
        self.cursor_pos = pos;
        match button {
            PointerButton::Primary => {
                let coord = self.camera.screen_to_world(pos);
                if modifiers.ctrl {
                    self.selection = Some((coord, coord));
                    self.is_selecting = true;
                    self.active_pattern = None;
                    self.paint_brush = None;
                } else if let Some(pat) = &self.active_pattern {
                    self.send_command(SimCommand::PlacePattern(coord, Arc::clone(&pat.cells)));
                    if !modifiers.shift {
                        self.active_pattern = None;
                        self.hud_dirty = true;
                    }
                } else {
                    self.selection = None;
                    let target_alive = !self.snapshot.cells.contains(&coord);
                    self.paint_brush = Some(target_alive);
                    self.last_painted = Some(coord);
                    let expanded = self.symmetry.expand(coord);
                    self.send_command(SimCommand::SetCells(self.symmetry.expand(coord), target_alive));
                }
            }
            PointerButton::Secondary => {
                if self.active_pattern.take().is_some() || self.selection.take().is_some() {
                    self.hud_dirty = true;
                } else {
                    self.is_panning = true;
                }
            }
            PointerButton::Middle => self.is_panning = true,
            _ => {}
        }
    }

    fn handle_release(&mut self, button: PointerButton) {
        match button {
            PointerButton::Primary => {
                self.paint_brush = None;
                self.last_painted = None;
                self.is_selecting = false;
            }
            PointerButton::Secondary | PointerButton::Middle => self.is_panning = false,
            _ => {}
        }
    }

    fn handle_key(&mut self, key: Key, ctrl: bool, window_center: Pos2) {
        let show_help = Cell::new(false);
        let next_help_open = !self.help_open;
        let needs_help_text = next_help_open && self.help_text.is_none();

        let mut dispatch_keys = |pressed: Option<(Key, bool)>, kb: &mut KeyRegistry| {
            // --- 时间与模拟控制 ---
            if kb.bind(pressed, (Key::Space, false), "时间控制", "Space", "暂停 / 继续演化") {
                self.send_command(SimCommand::TogglePause);
                self.status_toast = None;
            }
            if kb.bind(pressed, (Key::N, false), "时间控制", "N / →", "暂停并单步推演一代")
                || pressed == Some((Key::ArrowRight, false))
            {
                self.send_command(SimCommand::Step);
            }
            if kb.bind(pressed, (Key::Z, false), "时间控制", "Z / ←", "时光倒流回退一代")
                || pressed == Some((Key::ArrowLeft, false))
            {
                self.send_command(SimCommand::Rewind);
            }
            if kb.bind(pressed, (Key::Equals, false), "时间控制", "+ / ↑", "加快演化速度")
                || matches!(pressed, Some((Key::Plus | Key::ArrowUp, false)))
            {
                self.send_command(SimCommand::AdjustSpeed(0.75));
            }
            if kb.bind(pressed, (Key::Minus, false), "时间控制", "- / ↓", "减慢演化速度")
                || pressed == Some((Key::ArrowDown, false))
            {
                self.send_command(SimCommand::AdjustSpeed(1.33));
            }

            // --- 画布与编辑 ---
            if kb.bind(pressed, (Key::Home, false), "画布与编辑", "Home", "重置视角回到原点 (0, 0)") {
                self.camera.reset();
                self.hud_dirty = true;
            }
            if kb.bind(pressed, (Key::M, false), "画布与编辑", "M", "切换对称绘制模式") {
                self.symmetry = self.symmetry.next();
                self.hud_dirty = true;
            }
            if kb.bind(pressed, (Key::G, false), "画布与编辑", "G", "在屏幕中心生成 32x32 随机汤") {
                let center = self.camera.screen_to_world(window_center);
                let soup = generate_random_soup(center);
                self.send_command(SimCommand::SetCells(soup, true));
                self.status_toast = Some("Generated 32x32 Random Soup".into());
                self.hud_dirty = true;
            }
            if kb.bind(pressed, (Key::C, true), "画布与编辑", "Ctrl + C", "复制框选区域为图样印章") {
                if let Some(cells) = self.selection_cells(false) {
                    match Pattern::from_slice("Copied Selection", &cells) {
                        Ok(pat) => {
                            self.active_pattern = Some(pat);
                            self.selection = None;
                            self.status_toast =
                                Some(format!("Copied {} cells to stamp", cells.len()));
                            self.hud_dirty = true;
                        }
                        Err(msg) => {
                            self.status_toast = Some(format!("Copy failed: {msg}"));
                            self.hud_dirty = true;
                        }
                    }
                }
            }
            if kb.bind(pressed, (Key::S, true), "画布与编辑", "Ctrl + S", "导出框选或全图为 .rle 文件") {
                if let Some(cells) = self.selection_cells(true) {
                    match Pattern::from_slice("Exported Pattern", &cells) {
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
                            self.status_toast = Some(format!("Export failed: {msg}"));
                            self.hud_dirty = true;
                        }
                    }
                }
            }
            if kb.bind(pressed, (Key::C, false), "画布与编辑", "C", "清空当前画布") {
                self.selection = None;
                self.send_command(SimCommand::Clear);
            }

            // --- 图样印章 ---
            if kb.bind(pressed, (Key::Num1, false), "图样印章", "1 ~ 5", "选择内置预设图样")
                || matches!(
                    pressed,
                    Some((Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5, false)))
            {
                let idx = match pressed {
                    Some((Key::Num1, _)) => 0,
                    Some((Key::Num2, _)) => 1,
                    Some((Key::Num3, _)) => 2,
                    Some((Key::Num4, _)) => 3,
                    Some((Key::Num5, _)) => 4,
                    _ => 0,
                };
                if let Some(pat) = self.presets.get(idx) {
                    self.active_pattern = Some(pat.clone());
                    self.paint_brush = None;
                    self.hud_dirty = true;
                }
            }
            if kb.bind(pressed, (Key::R, false), "图样印章", "R", "顺时针旋转图样 90°"){
                if let Some(pat) = &mut self.active_pattern {
                    pat.rotate_cw();
                }
            }
            if kb.bind(pressed, (Key::F, false), "图样印章", "F", "水平镜像翻转图样"){
                if let Some(pat) = &mut self.active_pattern {
                    pat.flip_horizontal();
                }
            }
            if kb.bind(pressed, (Key::Backspace, false), "图样印章", "Backspace", "取消当前图样预览"){
                if self.active_pattern.take().is_some() {
                    self.hud_dirty = true;
                }
            }

            // --- 规则与系统 ---
            if kb.bind(
                pressed,
                (Key::F1, false),
                "规则与系统",
                "F1 ~ F4",
                "切换元胞规则 (Conway / HighLife / Seeds / Day&Night)",
            ) {
                self.send_command(SimCommand::SetRule(Rule::CONWAY));
            }
            if pressed == Some((Key::F2, false)) {
                self.send_command(SimCommand::SetRule(Rule::HIGH_LIFE));
            }
            if pressed == Some((Key::F3, false)) {
                self.send_command(SimCommand::SetRule(Rule::SEEDS));
            }
            if pressed == Some((Key::F4, false)) {
                self.send_command(SimCommand::SetRule(Rule::DAY_AND_NIGHT));
            }
            if kb.bind(pressed, (Key::F5, false), "规则与系统", "F5", "打开 / 关闭快捷键帮助窗口"){
                show_help.set(true);
            }
        };

        let mut live_kb = KeyRegistry::live();
        dispatch_keys(Some((key, ctrl)), &mut live_kb);

        if show_help.get() {
            if needs_help_text {
                let help_text = KeyRegistry::collect_help(dispatch_keys);
                self.help_text = Some(help_text);
            } else {
                drop(dispatch_keys);
            }
            self.help_open = next_help_open;
        }
    }

    fn update_status_text(&mut self) {
        if !self.hud_dirty {
            return;
        }

        let status = if self.snapshot.paused {
            "PAUSED"
        } else {
            "RUNNING"
        };
        let tps = 1000.0 / (self.snapshot.interval_ms.max(1) as f64);
        let sym_info = match self.symmetry {
            SymmetryMode::None => String::new(),
            mode => format!(" | Sym: {}", mode.label()),
        };
        let stamp_info = match &self.active_pattern {
            Some(pat) => format!(" | Stamp: {} [R:旋转 F:翻转 左键:放置 右键:取消]", pat.name),
            None => String::new(),
        };
        let toast_info = match &self.status_toast {
            Some(msg) => format!(" | ★ {}", msg),
            None => String::new(),
        };
        let status_text = format!(
            "[{}] | {} | Gen: {} | Pop: {} | {:.1} TPS{}{}{}",
            status,
            self.snapshot.rule.name,
            self.snapshot.generation,
            self.snapshot.cells.len(),
            tps,
            sym_info,
            stamp_info,
            toast_info
        );

        self.status_text = status_text;
        self.hud_dirty = false;
    }

    fn show_help_window(&mut self, ctx: &Context) {
        if !self.help_open {
            self.help_rect = None;
            return;
        }

        let mut open = self.help_open;
        let help_text = self.help_text.as_deref().unwrap_or_default();
        let response = egui::Window::new("快捷键帮助 (F5 / Esc 关闭)")
            .open(&mut open)
            .collapsible(false)
            .default_size([520.0, 560.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.monospace(help_text);
                    });
            });

        self.help_open = open;
        self.help_rect = response.map(|response| response.response.rect);
    }

    fn draw(&self, painter: &Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 31, 36));
        draw_grid(&self.camera, rect, painter);
        draw_symmetry_axes(&self.camera, self.symmetry, rect, painter);
        draw_cells(&self.camera, &self.snapshot.cells, rect, painter);

        if let Some(pat) = &self.active_pattern {
            let hover_coord = self.camera.screen_to_world(self.cursor_pos);
            draw_ghost_pattern(&self.camera, hover_coord, &pat.cells, painter);
        }
        if let Some(selection) = self.selection {
            draw_selection_box(&self.camera, selection, painter);
        }
    }
}

impl eframe::App for LifeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(latest) = self.state_rx.try_recv() {
            self.snapshot = latest;
            self.hud_dirty = true;
        }

        self.handle_file_drop(ui.ctx());
        self.update_status_text();

        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.label(self.status_text.as_str());
        });

        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            self.handle_events(ui.ctx(), rect);
            ui.allocate_rect(rect, Sense::click_and_drag());

            let painter = ui.painter_at(rect);
            self.draw(&painter, rect);
        });

        self.show_help_window(ui.ctx());
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn draw_grid(camera: &Camera, rect: Rect, painter: &Painter) {
    if camera.cell_size < 6.0 {
        return;
    }

    let (min_x, max_x, min_y, max_y) = camera.visible_bounds(rect);
    let grid_color = Color32::from_rgb(41, 43, 51);
    let axis_color = Color32::from_rgb(64, 69, 82);

    for wx in min_x..=max_x {
        let x = camera.world_to_screen((wx, 0)).x;
        let color = if wx == 0 { axis_color } else { grid_color };
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, color),
        );
    }
    for wy in min_y..=max_y {
        let y = camera.world_to_screen((0, wy)).y;
        let color = if wy == 0 { axis_color } else { grid_color };
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, color),
        );
    }
}

fn cell_rect(camera: &Camera, coord: Coord) -> Rect {
    let inset = if camera.cell_size >= 8.0 { 1.0 } else { 0.0 };
    let size = (camera.cell_size - inset * 2.0).max(1.0) as f32;
    Rect::from_min_size(
        camera.world_to_screen(coord) + vec2(inset as f32, inset as f32),
        vec2(size, size),
    )
}

fn draw_cells(camera: &Camera, cells: &HashSet<Coord>, rect: Rect, painter: &Painter) {
    let (min_x, max_x, min_y, max_y) = camera.visible_bounds(rect);
    let cell_color = Color32::from_rgb(56, 217, 140);

    for &coord in cells {
        if (min_x..=max_x).contains(&coord.0) && (min_y..=max_y).contains(&coord.1) {
            painter.rect_filled(cell_rect(camera, coord), 0.0, cell_color);
        }
    }
}

fn draw_ghost_pattern(camera: &Camera, (ox, oy): Coord, cells: &[Coord], painter: &Painter) {
    let ghost_color = Color32::from_rgba_unmultiplied(242, 191, 64, 115);
    for &(dx, dy) in cells {
        painter.rect_filled(cell_rect(camera, (ox + dx, oy + dy)), 0.0, ghost_color);
    }
}

fn draw_symmetry_axes(camera: &Camera, symmetry: SymmetryMode, rect: Rect, painter: &Painter) {
    if symmetry == SymmetryMode::None {
        return;
    }

    let color = Color32::from_rgba_unmultiplied(217, 89, 166, 166);
    let origin = camera.world_to_screen((0, 0));
    painter.line_segment(
        [pos2(origin.x, rect.top()), pos2(origin.x, rect.bottom())],
        Stroke::new(1.5, color),
    );
    if symmetry == SymmetryMode::Quad {
        painter.line_segment(
            [pos2(rect.left(), origin.y), pos2(rect.right(), origin.y)],
            Stroke::new(1.5, color),
        );
    }
}

fn draw_selection_box(camera: &Camera, (p1, p2): (Coord, Coord), painter: &Painter) {
    let min_x = p1.0.min(p2.0);
    let max_x = p1.0.max(p2.0) + 1;
    let min_y = p1.1.min(p2.1);
    let max_y = p1.1.max(p2.1) + 1;
    let rect = Rect::from_min_max(
        camera.world_to_screen((min_x, min_y)),
        camera.world_to_screen((max_x, max_y)),
    );

    painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(64, 153, 242, 51));
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(89, 179, 255, 217)),
        StrokeKind::Inside,
    );
}

fn setup_chinese_font(ctx: &egui::Context) {
    let font_paths = [
        "C:/Windows/Fonts/msyh.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    if let Some(font_data) = font_paths.iter().find_map(|p| std::fs::read(p).ok()) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cjk".to_owned(),
            Arc::new(egui::FontData::from_owned(font_data)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "cjk".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "cjk".to_owned());
        ctx.set_fonts(fonts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_event_extracts_selected_pattern() {
        let (_state_tx, state_rx) = crossbeam::channel::unbounded();
        let (cmd_tx, _cmd_rx) = crossbeam::channel::unbounded();
        let mut app = LifeApp::new(state_rx, cmd_tx);
        app.snapshot.cells = Arc::new(HashSet::from([(0, 0), (1, 0), (2, 0), (1, 1), (2, 2)]));
        app.selection = Some(((-2, -2), (2, 2)));

        let rect = Rect::from_min_size(Pos2::ZERO, vec2(960.0, 720.0));
        let mut input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        input.events.push(Event::Copy);
        let ctx = Context::default();
        let mut output = ctx.run_pass(input, |ctx| app.handle_events(ctx, rect));
        output.textures_delta.clear();

        assert_eq!(
            app.active_pattern.as_ref().map(|pat| pat.cells.len()),
            Some(5)
        );
        assert_eq!(app.status_toast.as_deref(), Some("Copied 5 cells to stamp"));
        assert_eq!(app.selection, None);
    }

    #[test]
    fn help_window_blocks_canvas_pointer() {
        let (_state_tx, state_rx) = crossbeam::channel::unbounded();
        let (cmd_tx, cmd_rx) = crossbeam::channel::unbounded();
        let mut app = LifeApp::new(state_rx, cmd_tx);
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(960.0, 720.0));
        app.help_open = true;
        app.help_rect = Some(rect);

        let mut input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        input.events.push(Event::PointerButton {
            pos: rect.center(),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        });
        let ctx = Context::default();
        let mut output = ctx.run_pass(input, |ctx| app.handle_events(ctx, rect));
        output.textures_delta.clear();

        assert!(cmd_rx.is_empty());
        assert_eq!(app.paint_brush, None);
    }

    #[test]
    fn zoom_keeps_the_hovered_world_cell_anchored() {
        let mut camera = Camera::new();
        let cursor = Pos2::new(196.0, 180.0);
        let coord = camera.screen_to_world(cursor);
        assert_eq!(coord, (6, 5));

        camera.zoom_at(cursor, 1.15);
        let anchored = camera.world_to_screen(coord);
        assert!((anchored.x - cursor.x).abs() < 0.001);
        assert!((anchored.y - cursor.y).abs() < 0.001);
    }
}
