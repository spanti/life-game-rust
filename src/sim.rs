use crate::universe::{Coord, Snapshot, Universe};
use crossbeam::channel::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub enum SimCommand {
    SetCell(Coord, bool),
    TogglePause,
    Clear,
    SetInterval(Duration),
}

pub struct Simulation {
    universe: Universe,
    paused: bool,
    interval: Duration,
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
            cmd_rx,
            state_tx
        }
    }

    pub fn run(mut self){
        let _ = self.state_tx.send(self.universe.snapshot(self.paused));
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
                        self.universe.tick();
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

    /// 处理控制指令，返回 true 表示世界状态发生变更需要重新渲染
    fn handle_command(&mut self, cmd: SimCommand, next_tick: &mut Instant) -> bool {
        match cmd {
            SimCommand::SetCell(coord, alive) => {
                self.universe.set_cell(coord, alive);
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
            SimCommand::Clear => {
                self.universe.clear();
                true
            }
            SimCommand::SetInterval(new_interval) => {
                self.interval = new_interval;
                *next_tick = Instant::now() + self.interval;
                false
            }
        }
    }

    fn publish_snapshot(&self){
        let _ = self.state_tx.try_send(self.universe.snapshot(self.paused));
    }
}