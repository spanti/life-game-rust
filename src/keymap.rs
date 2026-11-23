use eframe::egui::Key;

pub struct KeyRegistry {
    entries: Vec<(&'static str, &'static str, &'static str)>,
    recording: bool,
}

impl KeyRegistry {
    /// 正常按键分发模式
    pub fn live() -> Self {
        Self {
            entries: Vec::new(),
            recording: false,
        }
    }

    pub fn bind(
        &mut self,
        pressed: Option<(Key, bool)>,
        target: (Key, bool),
        category: &'static str,
        label: &'static str,
        desc: &'static str,
    ) -> bool {
        if self.recording {
            self.entries.push((category, label, desc));
            return false;
        }
        pressed == Some(target)
    }

    /// 关联函数：创建一个临时录制器空跑一遍闭包，不占用任何外部变量借用
    pub fn collect_help<F>(mut dispatch: F) -> String
    where
        F: FnMut(Option<(Key, bool)>, &mut Self),
    {
        let mut reg = Self {
            entries: Vec::new(),
            recording: true,
        };
        dispatch(None, &mut reg);

        let mut out = String::from("\n================ 快捷键列表 ================\n");
        let mut last_cat = "";
        for &(cat, label, desc) in &reg.entries {
            if cat != last_cat {
                out.push_str(&format!("\n【{}】\n", cat));
                last_cat = cat;
            }
            out.push_str(&format!("  {:<12} | {}\n", label, desc));
        }
        out.push_str("  Esc          | 关闭帮助窗口；未打开帮助时退出程序\n");
        out.push_str("\n【鼠标操作】\n");
        out.push_str("  左键点击/拖动 | 绘制或擦除细胞（预览模式下为放置图样）\n");
        out.push_str("  Shift + 左键  | 按住 Shift 连续放置图样\n");
        out.push_str("  Ctrl + 左键   | 框选矩形区域（配合 Ctrl+C / Ctrl+S）\n");
        out.push_str("  右键 / 中键   | 平移画布（预览/框选状态下右键取消）\n");
        out.push_str("  鼠标滚轮      | 以光标为中心缩放网格\n");
        out.push_str("============================================\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_matches_exact_input_and_recording_never_fires_actions() {
        let bind = |registry: &mut KeyRegistry, pressed, target| {
            registry.bind(pressed, target, "test", "C", "clear")
        };

        let mut live = KeyRegistry::live();
        assert!(bind(&mut live, Some((Key::C, false)), (Key::C, false)));
        assert!(bind(&mut live, Some((Key::C, true)), (Key::C, true)));
        assert!(!bind(&mut live, Some((Key::C, true)), (Key::C, false)));
        assert!(!bind(&mut live, Some((Key::S, false)), (Key::C, false)));

        let mut action_count = 0;
        let help = KeyRegistry::collect_help(|pressed, registry| {
            if bind(registry, pressed, (Key::C, false)) {
                action_count += 1;
            }
        });

        assert_eq!(action_count, 0);
        assert!(help.contains("【test】"));
        assert!(help.contains("  C            | clear\n"));
    }
}
