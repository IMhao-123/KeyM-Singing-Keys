use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 修饰键状态
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub cmd: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub opt: bool,
}

/// 静音快捷键组合
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    pub keycode: u16,
    pub cmd: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub opt: bool,
}

impl KeyCombo {
    pub fn new(keycode: u16, state: ModifierState) -> Self {
        KeyCombo {
            keycode,
            cmd: state.cmd,
            shift: state.shift,
            ctrl: state.ctrl,
            opt: state.opt,
        }
    }
}

/// 静音快捷键：按下指定组合键时不播放按键音效（统计仍会记录）。
///
/// 第一版裁掉了“静音时段”与“静音应用”，仅保留“组合键静音”这一独立能力，
/// 从原 `mute_rules` 模块拆出以便独立维护。
///
/// 持久化沿用 `mute_rules.json`：加载时显式忽略已裁掉的 `quiet_hours` /
/// `muted_apps` 字段（旧文件存在不阻止启动）；保存时以读-改-写方式保留这些
/// 旧字段，不破坏用户既有数据，便于下一版恢复时复用。
pub struct MuteShortcut {
    shortcut_combos: Mutex<HashSet<KeyCombo>>,
    modifier_state: Mutex<ModifierState>,
    presets_loaded: Mutex<bool>,
}

impl Default for MuteShortcut {
    fn default() -> Self {
        Self::new()
    }
}

impl MuteShortcut {
    pub fn new() -> Self {
        MuteShortcut {
            shortcut_combos: Mutex::new(HashSet::new()),
            modifier_state: Mutex::new(ModifierState::default()),
            presets_loaded: Mutex::new(false),
        }
    }

    /// 生产配置文件路径（统一走 data_paths，支持测试覆盖）。
    /// F5/FRB-008：数据根无法确定时传播可见错误，绝不回退相对路径——
    /// 相对 `mute_rules.json` 会在不可预测的当前工作目录读写配置。
    fn config_path() -> Result<PathBuf, String> {
        crate::data_paths::data_root()
            .map(|d| d.join("KeyM/mute_rules.json"))
            .map_err(|error| format!("静音快捷键配置路径不可用: {error}"))
    }

    /// 系统预设的常用 macOS 快捷键（13 个）
    pub fn default_presets() -> Vec<KeyCombo> {
        let cmd = ModifierState {
            cmd: true,
            shift: false,
            ctrl: false,
            opt: false,
        };
        let cmd_shift = ModifierState {
            cmd: true,
            shift: true,
            ctrl: false,
            opt: false,
        };
        let ctrl_cmd = ModifierState {
            cmd: true,
            shift: false,
            ctrl: true,
            opt: false,
        };
        vec![
            KeyCombo::new(0x08, cmd),       // Cmd+C
            KeyCombo::new(0x09, cmd),       // Cmd+V
            KeyCombo::new(0x07, cmd),       // Cmd+X
            KeyCombo::new(0x00, cmd),       // Cmd+A
            KeyCombo::new(0x06, cmd),       // Cmd+Z
            KeyCombo::new(0x30, cmd),       // Cmd+Tab
            KeyCombo::new(0x0C, cmd),       // Cmd+Q
            KeyCombo::new(0x0D, cmd),       // Cmd+W
            KeyCombo::new(0x14, cmd_shift), // Cmd+Shift+3
            KeyCombo::new(0x15, cmd_shift), // Cmd+Shift+4
            KeyCombo::new(0x17, cmd_shift), // Cmd+Shift+5
            KeyCombo::new(0x0C, ctrl_cmd),  // Ctrl+Cmd+Q (锁屏)
            KeyCombo::new(0x31, cmd),       // Cmd+Space (Spotlight)
        ]
    }

    /// 生产构造：从默认配置路径加载。
    /// F5/FRB-008：数据根不可解析时返回错误，由调用方决定内存降级并透出错误。
    pub fn load() -> Result<Self, String> {
        Ok(Self::load_from(&Self::config_path()?))
    }

    /// 显式路径构造（测试用，禁止指向真实用户目录）
    pub fn load_from(path: &Path) -> Self {
        let rules = Self::new();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(combos) = v.get("shortcut_combos").and_then(|x| x.as_array()) {
                        let mut set = rules.shortcut_combos.lock().unwrap();
                        for c in combos {
                            if let Ok(combo) = serde_json::from_value::<KeyCombo>(c.clone()) {
                                set.insert(combo);
                            }
                        }
                    }
                    if let Some(loaded) = v.get("presets_loaded").and_then(|x| x.as_bool()) {
                        *rules.presets_loaded.lock().unwrap() = loaded;
                    }
                    // quiet_hours / muted_apps 已随第一版裁掉，这里显式忽略：
                    // 旧文件中存在这些字段不阻止加载。
                }
            }
        }
        // 首次运行：装入预设
        if !*rules.presets_loaded.lock().unwrap() {
            rules.load_presets();
        }
        rules
    }

    /// 持久化到指定路径（读-改-写，保留已裁掉的 quiet_hours / muted_apps 旧字段）。
    /// AUD-020：原子写盘并返回 Result，失败时调用方可回滚内存并报告错误。
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let mut root = if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        let combos: Vec<KeyCombo> = self
            .shortcut_combos
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        let presets_loaded = *self.presets_loaded.lock().unwrap();
        let as_obj = root.as_object_mut();
        match as_obj {
            Some(obj) => {
                obj.insert(
                    "shortcut_combos".into(),
                    serde_json::to_value(&combos).unwrap_or_default(),
                );
                obj.insert(
                    "presets_loaded".into(),
                    serde_json::Value::Bool(presets_loaded),
                );
            }
            None => {
                root = serde_json::json!({
                    "shortcut_combos": combos,
                    "presets_loaded": presets_loaded,
                });
            }
        }
        let bytes =
            serde_json::to_vec_pretty(&root).map_err(|e| format!("序列化静音快捷键失败: {e}"))?;
        crate::atomic_file::write(path, &bytes).map_err(|e| format!("保存静音快捷键失败: {e}"))
    }

    /// 生产持久化。F5/FRB-008：数据根失败时传播错误，绝不写相对路径。
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::config_path()?)
    }

    /// 装入系统预设（不覆盖用户已有）
    pub fn load_presets(&self) {
        let mut set = self.shortcut_combos.lock().unwrap();
        for combo in Self::default_presets() {
            set.insert(combo);
        }
        *self.presets_loaded.lock().unwrap() = true;
    }

    /// 更新修饰键状态（由 event_tap 在 KeyDown/FlagsChanged 时调用）
    pub fn on_keycode(&self, keycode: u16, is_down: bool) {
        let mut state = self.modifier_state.lock().unwrap();
        match keycode {
            0x37 | 0x36 => state.cmd = is_down,   // L/R Cmd
            0x38 | 0x3C => state.shift = is_down, // L/R Shift
            0x3B | 0x3E => state.ctrl = is_down,  // L/R Ctrl
            0x3A | 0x3D => state.opt = is_down,   // L/R Opt
            _ => {}
        }
    }

    /// 用 FlagsChanged 事件的 flags 直接设置修饰键状态（更准确）
    pub fn set_modifier_state(&self, cmd: bool, shift: bool, ctrl: bool, opt: bool) {
        let mut state = self.modifier_state.lock().unwrap();
        state.cmd = cmd;
        state.shift = shift;
        state.ctrl = ctrl;
        state.opt = opt;
    }

    /// 当前修饰键状态
    pub fn current_modifiers(&self) -> ModifierState {
        *self.modifier_state.lock().unwrap()
    }

    /// 检查当前组合是否命中静音快捷键
    pub fn should_mute_combo(&self, keycode: u16) -> bool {
        let state = *self.modifier_state.lock().unwrap();
        // 无修饰键时不算组合
        if !state.cmd && !state.shift && !state.ctrl && !state.opt {
            return false;
        }
        let combo = KeyCombo::new(keycode, state);
        self.shortcut_combos.lock().unwrap().contains(&combo)
    }

    pub fn add_combo(&self, combo: KeyCombo) {
        self.shortcut_combos.lock().unwrap().insert(combo);
    }

    pub fn remove_combo(&self, combo: &KeyCombo) {
        self.shortcut_combos.lock().unwrap().remove(combo);
    }

    pub fn get_combos(&self) -> Vec<KeyCombo> {
        self.shortcut_combos
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    /// AUD-020：新增组合并原子持久化；写盘失败回滚内存并返回错误。
    /// F5/FRB-008：数据根失败时在变更内存前直接报错，不产生相对路径文件。
    pub fn add_combo_persisted(&self, combo: KeyCombo) -> Result<(), String> {
        self.add_combo_persisted_to(combo, &Self::config_path()?)
    }

    /// AUD-020：删除组合并原子持久化；写盘失败回滚内存并返回错误。
    /// F5/FRB-008：数据根失败时在变更内存前直接报错。
    pub fn remove_combo_persisted(&self, combo: &KeyCombo) -> Result<(), String> {
        self.remove_combo_persisted_to(combo, &Self::config_path()?)
    }

    /// AUD-020：装入系统预设并原子持久化；写盘失败回滚内存并返回错误。
    /// F5/FRB-008：数据根失败时在变更内存前直接报错。
    pub fn reset_presets_persisted(&self) -> Result<(), String> {
        self.reset_presets_persisted_to(&Self::config_path()?)
    }

    fn snapshot_state(&self) -> (HashSet<KeyCombo>, bool) {
        (
            self.shortcut_combos.lock().unwrap().clone(),
            *self.presets_loaded.lock().unwrap(),
        )
    }

    fn restore_state(&self, state: (HashSet<KeyCombo>, bool)) {
        *self.shortcut_combos.lock().unwrap() = state.0;
        *self.presets_loaded.lock().unwrap() = state.1;
    }

    fn add_combo_persisted_to(&self, combo: KeyCombo, path: &Path) -> Result<(), String> {
        let before = self.snapshot_state();
        self.add_combo(combo);
        if let Err(error) = self.save_to(path) {
            self.restore_state(before);
            return Err(error);
        }
        Ok(())
    }

    fn remove_combo_persisted_to(&self, combo: &KeyCombo, path: &Path) -> Result<(), String> {
        let before = self.snapshot_state();
        self.remove_combo(combo);
        if let Err(error) = self.save_to(path) {
            self.restore_state(before);
            return Err(error);
        }
        Ok(())
    }

    fn reset_presets_persisted_to(&self, path: &Path) -> Result<(), String> {
        let before = self.snapshot_state();
        self.load_presets();
        if let Err(error) = self.save_to(path) {
            self.restore_state(before);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_combo_no_mute() {
        let rules = MuteShortcut::new();
        // 无修饰键、无组合 -> 不静音
        assert!(!rules.should_mute_combo(0x08));
    }

    #[test]
    fn test_combo_mute_cmd_c() {
        let rules = MuteShortcut::new();
        rules.add_combo(KeyCombo::new(
            0x08,
            ModifierState {
                cmd: true,
                shift: false,
                ctrl: false,
                opt: false,
            },
        ));
        // 模拟按下 Cmd
        rules.on_keycode(0x37, true);
        // 按 C 应命中
        assert!(rules.should_mute_combo(0x08));
        // 普通 C（无修饰）不命中
        rules.on_keycode(0x37, false);
        assert!(!rules.should_mute_combo(0x08));
    }

    #[test]
    fn test_default_presets_count() {
        assert_eq!(MuteShortcut::default_presets().len(), 13);
    }

    #[test]
    fn test_modifier_state_reset() {
        let rules = MuteShortcut::new();
        rules.on_keycode(0x37, true); // Cmd down
        assert!(rules.current_modifiers().cmd);
        rules.on_keycode(0x37, false); // Cmd up
        assert!(!rules.current_modifiers().cmd);
    }

    #[test]
    fn test_set_modifier_state() {
        let rules = MuteShortcut::new();
        rules.set_modifier_state(true, true, false, false);
        let s = rules.current_modifiers();
        assert!(s.cmd && s.shift && !s.ctrl && !s.opt);
    }

    #[test]
    fn test_load_presets() {
        let rules = MuteShortcut::new();
        assert_eq!(rules.get_combos().len(), 0);
        rules.load_presets();
        assert_eq!(rules.get_combos().len(), 13);
        // Cmd+C 预设命中
        rules.set_modifier_state(true, false, false, false);
        assert!(rules.should_mute_combo(0x08));
    }

    /// TDD 安全网：load/save 往返保持组合（使用显式临时路径，禁止触碰真实用户目录）
    #[test]
    fn test_load_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "roundtrip"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mute_rules.json");
        std::fs::remove_file(&path).ok();

        // 模拟生产路径：load_from 首次运行会装入 13 个预设并标记 presets_loaded
        let rules = MuteShortcut::load_from(&path);
        assert_eq!(rules.get_combos().len(), 13);
        rules.add_combo(KeyCombo::new(
            0x09,
            ModifierState {
                cmd: true,
                shift: true, // Cmd+Shift+V：不在预设内，避免与预设去重
                ctrl: false,
                opt: false,
            },
        ));
        rules.save_to(&path).unwrap();

        let reloaded = MuteShortcut::load_from(&path);
        // 13 预设 + 1 自定义，重载不重复追加预设
        assert_eq!(reloaded.get_combos().len(), 14);
        reloaded.set_modifier_state(true, true, false, false);
        assert!(reloaded.should_mute_combo(0x09));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// TDD 安全网：旧 mute_rules.json 含已裁掉的 quiet_hours / muted_apps 字段时，
    /// 加载不得失败，且只读取 shortcut_combos / presets_loaded。
    #[test]
    fn test_load_ignores_pruned_fields() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "legacy"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mute_rules.json");
        // 模拟旧版本写入的文件：含已裁掉字段 + 一个组合
        std::fs::write(
            &path,
            r#"{
                "quiet_hours": {"start": 22, "end": 7},
                "muted_apps": ["WeChat", "Slack"],
                "shortcut_combos": [{"keycode": 8, "cmd": true, "shift": false, "ctrl": false, "opt": false}],
                "presets_loaded": true
            }"#,
        )
        .unwrap();

        // 加载不 panic
        let rules = MuteShortcut::load_from(&path);
        // 只读到 1 个组合，预设有标记（不再追加预设）
        assert_eq!(rules.get_combos().len(), 1);
        rules.set_modifier_state(true, false, false, false);
        assert!(rules.should_mute_combo(0x08));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// TDD 安全网：保存时以读-改-写保留已裁掉的旧字段，不破坏用户既有数据。
    #[test]
    fn test_save_preserves_pruned_fields() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "preserve"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mute_rules.json");
        std::fs::write(
            &path,
            r#"{
                "quiet_hours": {"start": 22, "end": 7},
                "muted_apps": ["WeChat"],
                "shortcut_combos": [],
                "presets_loaded": true
            }"#,
        )
        .unwrap();

        let rules = MuteShortcut::load_from(&path);
        rules.add_combo(KeyCombo::new(
            0x07,
            ModifierState {
                cmd: true,
                shift: false,
                ctrl: false,
                opt: false,
            },
        ));
        rules.save_to(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        // 旧字段被保留
        assert_eq!(v["quiet_hours"]["start"], 22);
        assert_eq!(v["muted_apps"][0], "WeChat");
        // 新组合被写入
        assert_eq!(v["shortcut_combos"].as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// AUD-020：持久化失败时回滚内存并返回错误，不留半提交状态。
    #[test]
    fn test_persisted_mutation_rolls_back_on_save_failure() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "rollback"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 用一个普通文件充当“目录”，使 create_dir_all/写入必然失败
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not a dir").unwrap();
        let bad_path = blocker.join("mute_rules.json");

        let rules = MuteShortcut::new();
        let combo = KeyCombo::new(
            0x08,
            ModifierState {
                cmd: true,
                shift: false,
                ctrl: false,
                opt: false,
            },
        );
        assert!(rules.add_combo_persisted_to(combo, &bad_path).is_err());
        assert!(rules.get_combos().is_empty(), "写盘失败后内存必须回滚");

        rules.add_combo(combo);
        assert!(rules.remove_combo_persisted_to(&combo, &bad_path).is_err());
        assert_eq!(rules.get_combos().len(), 1, "删除写盘失败后内存必须回滚");

        assert!(rules.reset_presets_persisted_to(&bad_path).is_err());
        assert_eq!(rules.get_combos().len(), 1, "预设写盘失败后内存必须回滚");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 验收观察项 O2：旧 mute_rules.json 为损坏 JSON 时，
    /// 加载走"解析失败→忽略→装预设"路径，不 panic、不丢可用状态。
    #[test]
    fn test_load_corrupted_json_falls_back_to_presets() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "corrupted"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mute_rules.json");
        // 截断的损坏 JSON
        std::fs::write(&path, r#"{"shortcut_combos": [{"keycode": 8, "cm"#).unwrap();

        let rules = MuteShortcut::load_from(&path);
        // 解析失败被忽略，按首次运行装入 13 条预设
        assert_eq!(rules.get_combos().len(), 13);
        rules.set_modifier_state(true, false, false, false);
        assert!(rules.should_mute_combo(0x08));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// KEYM_TEST_DATA_DIR 覆盖守卫：测试结束恢复原值，避免污染同进程其他测试。
    /// （质量门以 --test-threads=1 串行运行，测试内修改环境变量安全。）
    struct TestDataDirGuard(Option<std::ffi::OsString>);

    impl TestDataDirGuard {
        fn set(value: &str) -> Self {
            let previous = std::env::var_os(crate::data_paths::TEST_DATA_DIR_ENV);
            std::env::set_var(crate::data_paths::TEST_DATA_DIR_ENV, value);
            Self(previous)
        }
    }

    impl Drop for TestDataDirGuard {
        fn drop(&mut self) {
            match &self.0 {
                Some(previous) => std::env::set_var(crate::data_paths::TEST_DATA_DIR_ENV, previous),
                None => std::env::remove_var(crate::data_paths::TEST_DATA_DIR_ENV),
            }
        }
    }

    /// F5/FRB-008：数据根失败（非法 override：相对路径 / 空值）时，
    /// config_path 与全部生产操作传播可见错误，且绝不在当前工作目录
    /// 创建相对 mute_rules.json，内存状态保持不变。
    #[test]
    fn data_root_failure_propagates_error_without_relative_fallback() {
        let cwd_file = std::env::current_dir().unwrap().join("mute_rules.json");
        assert!(
            !cwd_file.exists(),
            "测试开始前当前目录不得存在 mute_rules.json 残留: {cwd_file:?}"
        );

        for bad_override in ["relative/path", ""] {
            let _guard = TestDataDirGuard::set(bad_override);

            let path_error =
                MuteShortcut::config_path().expect_err("非法数据根必须报错，不得回退相对路径");
            assert!(
                !path_error.is_empty(),
                "错误信息必须可读（生产启动/操作可见失败）"
            );
            assert!(
                MuteShortcut::load().is_err(),
                "数据根失败时生产构造必须报错（override={bad_override:?}）"
            );

            let rules = MuteShortcut::new();
            let combo = KeyCombo::new(
                0x08,
                ModifierState {
                    cmd: true,
                    shift: false,
                    ctrl: false,
                    opt: false,
                },
            );
            assert!(rules.add_combo_persisted(combo).is_err());
            assert!(
                rules.get_combos().is_empty(),
                "数据根失败时不得改动内存状态，也不得持久化"
            );
            assert!(rules.save().is_err());
            assert!(rules.reset_presets_persisted().is_err());
            assert!(
                rules.get_combos().is_empty(),
                "reset 失败同样不得改动内存状态"
            );

            assert!(
                !cwd_file.exists(),
                "当前目录绝不生成相对 mute_rules.json（override={bad_override:?}）"
            );
        }
    }

    /// F5/FRB-008 对照：合法绝对 override 下生产路径落在数据根内，
    /// 操作正常持久化，不触碰当前目录。
    #[test]
    fn valid_absolute_override_uses_data_root() {
        let dir = std::env::temp_dir().join(format!(
            "keym_shortcut_test_{}_{}",
            std::process::id(),
            "override"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let _guard = TestDataDirGuard::set(dir.to_str().unwrap());

        let path = MuteShortcut::config_path().unwrap();
        assert_eq!(path, dir.join("KeyM/mute_rules.json"));

        let rules = MuteShortcut::load().unwrap();
        assert_eq!(rules.get_combos().len(), 13);
        rules.save().unwrap();
        assert!(path.exists(), "合法数据根下应正常写入配置文件");
        assert!(
            !std::env::current_dir()
                .unwrap()
                .join("mute_rules.json")
                .exists(),
            "合法数据根下也不得在当前目录生成相对文件"
        );

        drop(_guard);
        std::fs::remove_dir_all(&dir).ok();
    }
}
