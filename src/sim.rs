use crate::universe::{Coord, Rule, Snapshot, Universe};
use crossbeam::channel::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::collections::VecDeque;

const MAX_HISTORY: usize = 200;
const MIN_INTERVAL: Duration = Duration::from_millis(5);
const MAX_INTERVAL: Duration = Duration::from_millis(1000);

pub enum SimCommand {
    SetCells(Vec<Coord>, bool), // 新增：批量设置细胞（对称画笔 / 随机汤）
    PlacePattern(Coord, Arc<Vec<Coord>>), // 新增：批量放置图样
    TogglePause,
    Step,              // 单步推演一代
    Rewind,            // 时光倒流一步
    Clear,
    // SetInterval(Duration),
    AdjustSpeed(f64),  // 速度倍率调整（如 0.75 加速，1.33 减速）
    SetRule(Rule),     // 切换生命规则
}

pub struct Simulation {
    universe: Universe,
    paused: bool,
    interval: Duration,
    history: VecDeque<Snapshot>,
    cmd_rx: Receiver<SimCommand>,
    state_tx: Sender<Snapshot>
}

impl Simulation{
    pub fn new(
        universe: Universe,
        interval: Duration,
        cmd_rx: Receiver<SimCommand>,
        state_tx: Sender<Snapshot>
    ) -> Self {
        Self {
            universe,
            paused: false,
            interval,
            history: VecDeque::with_capacity(MAX_HISTORY),
            cmd_rx,
            state_tx
        }
    }

    pub fn run(mut self){
        self.publish_snapshot();
        let mut next_tick = Instant::now() + self.interval;

        loop {
            if self.paused {
                match self.cmd_rx.recv() {
                    Ok(cmd) => {
                        if self.handle_command(cmd, &mut next_tick){
                            self.publish_snapshot();
                        }
                    }
                    Err(_) => break,
                }
            }
            else {
                match self.cmd_rx.recv_deadline(next_tick){
                    Ok(cmd) => {
                        if self.handle_command(cmd, &mut next_tick) {
                            self.publish_snapshot();
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        self.advance_one_generation();
                        self.publish_snapshot();

                        // 推进下一次截止时间，并防止卡顿后出现“追赶式连跳”
                        let now = Instant::now();
                        next_tick += self.interval;
                        if next_tick < now {
                            next_tick = now + self.interval;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }
    }

    fn advance_one_generation(&mut self) {
        self.update_history();
        self.universe.tick();
    }

    fn update_history(&mut self) {
        if self.history.len() == MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(self.current_snapshot());
    }

    /// 处理控制指令，返回 true 表示世界状态发生变更需要重新渲染
    fn handle_command(&mut self, cmd: SimCommand, next_tick: &mut Instant) -> bool {
        match cmd {
            SimCommand::SetCells(coords, alive) => {
                // 若一次性批量生成较多细胞（如随机汤），先存入历史栈方便按 Z 撤回
                if coords.len() > 8 {
                    self.update_history();
                }
                self.universe.set_cells(coords, alive);
                true
            }
            SimCommand::TogglePause => {
                self.paused = !self.paused;
                // 取消暂停时，从当前时刻重新计算下一帧节奏
                if !self.paused {
                    *next_tick = Instant::now() + self.interval;
                }
                false
            }
            SimCommand::Step => {
                // 单步调试时自动进入暂停状态，方便逐帧观察
                self.paused = true;
                self.advance_one_generation();
                true
            }
            SimCommand::Rewind => {
                if let Some(prev) = self.history.pop_back() {
                    // 回退时自动暂停，防止刚退回去又立刻被定时器往前推走
                    self.paused = true;
                    self.universe.restore(&prev);
                    true
                } else {
                    false
                }
            }
            SimCommand::Clear => {
                self.update_history();
                self.universe.clear();
                true
            }
            SimCommand::AdjustSpeed(factor) => {
                let new_secs = (self.interval.as_secs_f64() * factor)
                    .clamp(MIN_INTERVAL.as_secs_f64(), MAX_INTERVAL.as_secs_f64());
                self.interval = Duration::from_secs_f64(new_secs);
                *next_tick = Instant::now() + self.interval;
                true
            }
            SimCommand::SetRule(rule) => {
                self.universe.set_rule(rule);
                true
            }
            SimCommand::PlacePattern(origin, cells) => {
                self.update_history();
                self.universe.place_pattern(origin, &cells);
                true
            }
        }
    }

    fn current_snapshot(&self) -> Snapshot {
        self.universe
            .snapshot(self.paused, self.interval.as_millis() as u64)
    }

    fn publish_snapshot(&self){
        let _ = self.state_tx.try_send(self.current_snapshot());
    }
}