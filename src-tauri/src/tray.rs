//! 托盘"音效开关"菜单项的展示投影（F3/AUD-024）。
//!
//! 单一状态源是 `AudioEngine::is_enabled()`；托盘菜单的文案/图标只是它的投影。
//! 任何入口（托盘 toggle、主窗口开关、IPC `toggle_sound`）改变状态后都会广播
//! `sound-state-changed`，main.rs 里唯一的事件监听器读取引擎真实状态并调用
//! 这里的纯函数刷新托盘，保证窗口→托盘方向也闭环。

/// 事件名：音效开关状态变化（负载为最新布尔值，刷新时仍以引擎状态为准）。
pub const SOUND_STATE_CHANGED_EVENT: &str = "sound-state-changed";

/// 托盘菜单项文案。
pub fn sound_toggle_label(enabled: bool) -> &'static str {
    if enabled {
        "音效: 开启"
    } else {
        "音效: 关闭"
    }
}

/// 托盘菜单项使用开启还是关闭图标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundToggleIcon {
    On,
    Off,
}

pub fn sound_toggle_icon(enabled: bool) -> SoundToggleIcon {
    if enabled {
        SoundToggleIcon::On
    } else {
        SoundToggleIcon::Off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F3/AUD-024：托盘投影与引擎状态一一对应，开/关两态文案与图标互不相同。
    #[test]
    fn toggle_projection_matches_enabled_state() {
        assert_eq!(sound_toggle_label(true), "音效: 开启");
        assert_eq!(sound_toggle_label(false), "音效: 关闭");
        assert_ne!(sound_toggle_label(true), sound_toggle_label(false));
        assert_eq!(sound_toggle_icon(true), SoundToggleIcon::On);
        assert_eq!(sound_toggle_icon(false), SoundToggleIcon::Off);
        assert_ne!(sound_toggle_icon(true), sound_toggle_icon(false));
    }
}
